// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

//! An Actor that listens on a TCP socket to accept incoming sessions.
//!
//! See [ListenerStartupArgs] for its startup arguments.

use ractor::{Actor, ActorProcessingErr, ActorRef};
use std::marker::PhantomData;
use tokio::net::TcpListener;

use super::{IncomingEncryptionMode, NetworkStream};

#[cfg_attr(feature = "async-trait", async_trait::async_trait)]
pub trait SessionAcceptor: ractor::State {
    /// Called when a new incoming session is received by a [Listener] actor
    #[cfg(not(feature = "async-trait"))]
    fn new_session(
        &self,
        session: NetworkStream,
    ) -> impl std::future::Future<Output = Result<(), ActorProcessingErr>> + Send;

    /// Called when a new incoming session is received by a [Listener] actor
    #[cfg(feature = "async-trait")]
    async fn new_session(&self, session: NetworkStream) -> Result<(), ActorProcessingErr>;
}

/// A Tcp Socket [Listener] responsible for creating the socket and accepting new connections. When
/// a client connects, [SessionAcceptor::new_session] will be called.
///
/// The callback can create a new [super::session::TcpSession] which handle the packet sending and
/// receiving over the socket.
pub struct Listener<R>
where
    R: SessionAcceptor,
{
    _r: PhantomData<fn() -> R>,
}

impl<R> Default for Listener<R>
where
    R: SessionAcceptor,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<R> Listener<R>
where
    R: SessionAcceptor,
{
    /// Create a new TCP Listener actor
    pub fn new() -> Self {
        Self { _r: PhantomData }
    }
}

/// The Listener's state
pub struct ListenerState<R>
where
    R: SessionAcceptor,
{
    listener: Option<TcpListener>,
    acceptor: R,
    encryption: IncomingEncryptionMode,
}

/// Arguments to startup a TcpListener
pub struct ListenerStartupArgs<R>
where
    R: SessionAcceptor,
{
    /// Port to listen on
    pub port: super::NetworkPort,
    /// Encryption settings for incoming sockets
    pub encryption: IncomingEncryptionMode,
    /// Callback module for accepted sockets
    pub acceptor: R,
}

pub struct ListenerMessage;

#[cfg_attr(feature = "async-trait", ractor::async_trait)]
impl<R> Actor for Listener<R>
where
    R: SessionAcceptor,
{
    type Msg = ListenerMessage;
    type State = ListenerState<R>;
    type Arguments = ListenerStartupArgs<R>;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let addr = format!("[::]:{}", args.port);
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
            acceptor: args.acceptor,
            encryption: args.encryption,
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

                    let stream = match &state.encryption {
                        IncomingEncryptionMode::Raw => Some(NetworkStream::Raw {
                            peer_addr: addr,
                            local_addr: local,
                            stream,
                        }),
                        IncomingEncryptionMode::Tls(acceptor) => {
                            match acceptor.accept(stream).await {
                                Ok(enc_stream) => Some(NetworkStream::TlsServer {
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

                    if let Some(stream) = stream {
                        state.acceptor.new_session(stream).await?;
                        tracing::info!("TCP Session opened for {}", addr);
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
