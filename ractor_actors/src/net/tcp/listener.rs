// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

//! Tcp listener actor

use std::marker::PhantomData;

use ractor::ActorProcessingErr;
use ractor::{Actor, ActorRef};
use tokio::net::TcpListener;

use super::{IncomingEncryptionMode, SessionAcceptor};

/// A Tcp Socket [Listener] responsible for accepting new connections.
#[derive(Default)]
pub struct Listener<R> {
    _r: PhantomData<R>,
}

/// The Node listener's state
pub struct ListenerState<R>
where
    R: SessionAcceptor,
{
    listener: Option<TcpListener>,
    acceptor: R,
    encryption: IncomingEncryptionMode,
}

pub struct TcpListenerStartupArgs<R>
where
    R: SessionAcceptor,
{
    port: super::NetworkPort,
    encryption: IncomingEncryptionMode,
    acceptor: R,
}

pub struct ListenerMessage;

#[async_trait::async_trait]
impl<R> Actor for Listener<R>
where
    R: SessionAcceptor,
{
    type Msg = ListenerMessage;
    type Arguments = TcpListenerStartupArgs<R>;
    type State = ListenerState<R>;

    async fn pre_start(
        &self,
        myself: ActorRef<Self>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let addr = format!("0.0.0.0:{}", args.port);
        let listener = match TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(err) => {
                return Err(From::from(err));
            }
        };

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
        _myself: ActorRef<Self>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // close the listener properly, in case anyone else has handles to the actor stopping
        // total droppage
        drop(state.listener.take());
        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self>,
        _message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        if let Some(listener) = &mut state.listener {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    let local = stream.local_addr()?;

                    let session = match &state.encryption {
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
                                    log::warn!("Error establishing secure socket: {}", some_err);
                                    None
                                }
                            }
                        }
                    };

                    if let Some(stream) = session {
                        state.acceptor.new_session(stream).await?;
                        log::info!("TCP Session opened for {}", addr);
                    }
                }
                Err(socket_accept_error) => {
                    log::warn!(
                        "Error accepting socket {} on Node server",
                        socket_accept_error
                    );
                }
            }
        }

        // continue accepting new sockets
        let _ = myself.cast(ListenerMessage);
        Ok(())
    }
}
