// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

//! TCP Server to accept incoming sessions

use ractor::ActorProcessingErr;
use ractor::{Actor, ActorRef};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::net::TcpListener;

use super::{IncomingEncryptionMode, NetworkStream};

/// A Tcp Socket [Listener] responsible for creating the socket and accepting new connections. When
/// a client connects, the `on_connection` will be called. The callback should create a new
/// [super::session::Session]s which handle the message sending and receiving over the socket.
///
/// The [Listener] supervises all the TCP [super::session::Session] actors and is responsible for
/// logging connects and disconnects.
pub struct Listener {
    port: super::NetworkPort,
    on_connection: Arc<
        dyn Fn(
                NetworkStream,
            ) -> Pin<Box<dyn Future<Output = Result<(), ActorProcessingErr>> + Send>>
            + Send
            + Sync,
    >,

    encryption: IncomingEncryptionMode,
}

impl Listener {
    /// Create a new `Listener`
    pub fn new<F, Fut>(
        port: super::NetworkPort,
        on_connection: F,
        encryption: IncomingEncryptionMode,
    ) -> Self
    where
        F: Fn(NetworkStream) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), ActorProcessingErr>> + Send + 'static,
    {
        Self {
            port,
            on_connection: Arc::new(move |stream| Box::pin(on_connection(stream))),
            encryption,
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
                        let _ = (self.on_connection)(stream).await?;
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
}
