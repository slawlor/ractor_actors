// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

//! TCP Server to accept incoming sessions

use ractor::{Actor, ActorRef, DerivedActorRef, SupervisionEvent};
use ractor::{ActorCell, ActorProcessingErr};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::net::TcpListener;

use super::{FrameAvailable, IncomingEncryptionMode, NetworkStreamInfo, SendFrame, Session};

/// A Tcp Socket [Listener] responsible for creating the socket and accepting new connections. When
/// a client connects, the `on_connection` will be called. The callback should create a new
/// [super::session::Session]s which handle the message sending and receiving over the socket.
///
/// The [Listener] supervises all the TCP [super::session::Session] actors and is responsible for
/// logging connects and disconnects.
pub struct Listener {
    port: super::NetworkPort,
    encryption: IncomingEncryptionMode,

    spawn_handler: Arc<
        dyn Fn(
                ActorCell,
                DerivedActorRef<SendFrame>,
                NetworkStreamInfo,
            ) -> Pin<
                Box<
                    dyn Future<Output = Result<DerivedActorRef<FrameAvailable>, ActorProcessingErr>>
                        + Send,
                >,
            > + Send
            + Sync,
    >,
}

impl Listener {
    /// Create a new `Listener`
    pub fn new<F, Fut>(
        port: super::NetworkPort,
        encryption: IncomingEncryptionMode,
        spawn_handler: F,
    ) -> Self
    where
        F: Fn(ActorCell, DerivedActorRef<SendFrame>, NetworkStreamInfo) -> Fut
            + Send
            + Sync
            + 'static,
        Fut: Future<Output = Result<DerivedActorRef<FrameAvailable>, ActorProcessingErr>>
            + Send
            + 'static,
    {
        Self {
            port,
            encryption,
            spawn_handler: Arc::new(move |cell, session, info| {
                Box::pin(spawn_handler(cell, session, info))
            }),
        }
    }
}

/// The Listener's state
pub struct ListenerState {
    listener: Option<TcpListener>,
}

pub struct ListenerMessage;

#[cfg_attr(feature = "async-trait", ractor::async_trait)]
impl Actor for Listener {
    type Msg = ListenerMessage;
    type Arguments = ();
    type State = ListenerState;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        _: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let addr = format!("[::]:{}", self.port);
        let listener = match TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(err) => {
                return Err(From::from(err));
            }
        };

        tracing::trace!("Listening on {}", addr);

        // startup the event processing loop by sending an initial msg
        let _ = myself.cast(ListenerMessage);

        // create the initial state
        Ok(Self::State {
            listener: Some(listener),
        })
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // close the listener properly, in case anyone else has handles to the actor stopping
        // total droppage
        drop(state.listener.take());
        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        _message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        if let Some(listener) = &mut state.listener {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    let local = stream.local_addr()?;

                    let session = match &self.encryption {
                        IncomingEncryptionMode::Raw => Some(super::NetworkStream::Raw {
                            peer_addr: addr,
                            local_addr: local,
                            stream,
                        }),
                        IncomingEncryptionMode::Tls(acceptor) => {
                            match acceptor.accept(stream).await {
                                Ok(enc_stream) => Some(super::NetworkStream::TlsServer {
                                    peer_addr: addr,
                                    local_addr: local,
                                    stream: enc_stream,
                                }),
                                Err(some_err) => {
                                    tracing::warn!("Error establishing secure socket: {some_err}");
                                    None
                                }
                            }
                        }
                    };

                    if let Some(stream) = session {
                        let _ = Actor::spawn_linked(
                            Some(addr.to_string()),
                            Session {
                                spawn_handler: self.spawn_handler.clone(),
                                peer_addr: stream.peer_addr(),
                                local_addr: stream.local_addr(),
                            },
                            stream,
                            myself.get_cell(),
                        )
                        .await?;
                        tracing::info!("TCP Session opened for {addr}");
                    }
                }
                Err(socket_accept_error) => {
                    tracing::warn!("Error accepting socket {socket_accept_error}");
                }
            }
        }

        // continue accepting new sockets
        let _ = myself.cast(ListenerMessage);
        Ok(())
    }

    async fn handle_supervisor_evt(
        &self,
        _: ActorRef<Self::Msg>,
        message: SupervisionEvent,
        _: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // Ignore any supervisor messages to the listener
        match message {
            _ => Ok(())
        }
    }
}
