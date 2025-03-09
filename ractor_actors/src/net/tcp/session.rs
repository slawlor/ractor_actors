// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

//! Tcp session managing actor

use std::marker::PhantomData;

use ractor::{Actor, ActorRef, SupervisionEvent};
use tokio::io::ErrorKind;
use tokio::io::{AsyncReadExt, ReadHalf, WriteHalf};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;

use super::*;

/// Supported messages by the [TcpSession] actor
pub enum TcpSessionMessage {
    /// Send a frame over the wire
    Send(Frame),
    /// Received a frame from the `SessionReader` child
    FrameReady(Frame),
}

/// State of a [TcpSession] actor
pub struct TcpSessionState<R>
where
    R: FrameReceiver,
{
    writer: ActorRef<SessionWriterMessage>,
    reader: ActorRef<SessionReaderMessage>,
    receiver: R,
    peer_addr: SocketAddr,
    local_addr: SocketAddr,
}

/// Startup arguments to starting a tcp session
pub struct TcpSessionStartupArguments<R>
where
    R: FrameReceiver,
{
    /// The callback implementation for received for messages
    pub receiver: R,
    /// The tcp session to creat the session upon
    pub tcp_session: NetworkStream,
}

/// A tcp-session management actor
pub struct TcpSession<R>
where
    R: FrameReceiver,
{
    _r: PhantomData<fn() -> R>,
}

impl<R> Default for TcpSession<R>
where
    R: FrameReceiver,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<R> TcpSession<R>
where
    R: FrameReceiver,
{
    /// Create a new TcpSession actor instance
    pub fn new() -> Self {
        Self { _r: PhantomData }
    }
}

#[cfg_attr(feature = "async-trait", async_trait::async_trait)]
impl<R> Actor for TcpSession<R>
where
    R: FrameReceiver,
{
    type Msg = TcpSessionMessage;
    type State = TcpSessionState<R>;
    type Arguments = TcpSessionStartupArguments<R>;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (peer_addr, local_addr) = (args.tcp_session.peer_addr(), args.tcp_session.local_addr());

        let (read, write) = match args.tcp_session {
            super::NetworkStream::Raw { stream, .. } => {
                let (read, write) = stream.into_split();
                (ActorReadHalf::Regular(read), ActorWriteHalf::Regular(write))
            }
            super::NetworkStream::TlsClient { stream, .. } => {
                let (read_half, write_half) = tokio::io::split(stream);
                (
                    ActorReadHalf::ClientTls(read_half),
                    ActorWriteHalf::ClientTls(write_half),
                )
            }
            super::NetworkStream::TlsServer { stream, .. } => {
                let (read_half, write_half) = tokio::io::split(stream);
                (
                    ActorReadHalf::ServerTls(read_half),
                    ActorWriteHalf::ServerTls(write_half),
                )
            }
        };

        // let (read, write) = stream.into_split();
        // spawn writer + reader child actors
        let (writer, _) =
            Actor::spawn_linked(None, SessionWriter, write, myself.get_cell()).await?;
        let (reader, _) = Actor::spawn_linked(
            None,
            SessionReader {
                session: myself.clone(),
            },
            read,
            myself.get_cell(),
        )
        .await?;

        Ok(Self::State {
            writer,
            reader,
            receiver: args.receiver,
            peer_addr,
            local_addr,
        })
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        tracing::info!("TCP Session closed for {}", state.peer_addr);
        Ok(())
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            Self::Msg::Send(msg) => {
                tracing::debug!(
                    "SEND: {} -> {} - '{:?}'",
                    state.local_addr,
                    state.peer_addr,
                    msg
                );
                let _ = state.writer.cast(SessionWriterMessage::Write(msg));
            }
            Self::Msg::FrameReady(msg) => {
                tracing::debug!(
                    "RECEIVE {} <- {} - '{:?}'",
                    state.local_addr,
                    state.peer_addr,
                    msg
                );
                state.receiver.frame_ready(msg).await?;
            }
        }
        Ok(())
    }

    async fn handle_supervisor_evt(
        &self,
        myself: ActorRef<Self::Msg>,
        message: SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // sockets open, they close, the world goes round... If a reader or writer exits for any reason, we'll start the shutdown procedure
        // which requires that all actors exit
        match message {
            SupervisionEvent::ActorFailed(actor, panic_msg) => {
                if actor.get_id() == state.reader.get_id() {
                    tracing::error!("TCP Session's reader panicked with '{}'", panic_msg);
                } else if actor.get_id() == state.writer.get_id() {
                    tracing::error!("TCP Session's writer panicked with '{}'", panic_msg);
                } else {
                    tracing::error!("TCP Session received a child panic from an unknown child actor ({}) - '{}'", actor.get_id(), panic_msg);
                }
                myself.stop(Some("child_panic".to_string()));
            }
            SupervisionEvent::ActorTerminated(actor, _, exit_reason) => {
                if actor.get_id() == state.reader.get_id() {
                    tracing::debug!("TCP Session's reader exited");
                } else if actor.get_id() == state.writer.get_id() {
                    tracing::debug!("TCP Session's writer exited");
                } else {
                    tracing::warn!("TCP Session received a child exit from an unknown child actor ({}) - '{:?}'", actor.get_id(), exit_reason);
                }
                myself.stop(Some("child_terminate".to_string()));
            }
            _ => {
                // all ok
            }
        }
        Ok(())
    }
}

// =========== Tcp stream types + helpers ============ //

enum ActorWriteHalf {
    ServerTls(WriteHalf<tokio_rustls::server::TlsStream<TcpStream>>),
    ClientTls(WriteHalf<tokio_rustls::client::TlsStream<TcpStream>>),
    Regular(OwnedWriteHalf),
}

impl ActorWriteHalf {
    async fn write_u64(&mut self, n: u64) -> tokio::io::Result<()> {
        use tokio::io::AsyncWriteExt;
        match self {
            Self::ServerTls(t) => t.write_u64(n).await,
            Self::ClientTls(t) => t.write_u64(n).await,
            Self::Regular(t) => t.write_u64(n).await,
        }
    }

    async fn write_all(&mut self, data: &[u8]) -> tokio::io::Result<()> {
        use tokio::io::AsyncWriteExt;
        match self {
            Self::ServerTls(t) => t.write_all(data).await,
            Self::ClientTls(t) => t.write_all(data).await,
            Self::Regular(t) => t.write_all(data).await,
        }
    }

    async fn flush(&mut self) -> tokio::io::Result<()> {
        use tokio::io::AsyncWriteExt;
        match self {
            Self::ServerTls(t) => t.flush().await,
            Self::ClientTls(t) => t.flush().await,
            Self::Regular(t) => t.flush().await,
        }
    }
}

enum ActorReadHalf {
    ServerTls(ReadHalf<tokio_rustls::server::TlsStream<TcpStream>>),
    ClientTls(ReadHalf<tokio_rustls::client::TlsStream<TcpStream>>),
    Regular(OwnedReadHalf),
}

impl ActorReadHalf {
    async fn read_u64(&mut self) -> tokio::io::Result<u64> {
        match self {
            Self::ServerTls(t) => t.read_u64().await,
            Self::ClientTls(t) => t.read_u64().await,
            Self::Regular(t) => t.read_u64().await,
        }
    }
}

/// Helper method to read exactly `len` bytes from the stream into a pre-allocated buffer
/// of bytes
async fn read_n_bytes(stream: &mut ActorReadHalf, len: usize) -> Result<Vec<u8>, tokio::io::Error> {
    let mut buf = vec![0u8; len];
    let mut c_len = 0;
    if let ActorReadHalf::Regular(r) = stream {
        r.readable().await?;
    }

    while c_len < len {
        let n = match stream {
            ActorReadHalf::ServerTls(t) => t.read(&mut buf[c_len..]).await?,
            ActorReadHalf::ClientTls(t) => t.read(&mut buf[c_len..]).await?,
            ActorReadHalf::Regular(t) => t.read(&mut buf[c_len..]).await?,
        };
        if n == 0 {
            // EOF
            return Err(tokio::io::Error::new(
                tokio::io::ErrorKind::UnexpectedEof,
                "EOF",
            ));
        }
        c_len += n;
    }
    Ok(buf)
}

// =========== Tcp Session writer ============ //

struct SessionWriter;

struct SessionWriterState {
    writer: Option<ActorWriteHalf>,
}

enum SessionWriterMessage {
    /// Write an object over the wire
    Write(Frame),
}

#[cfg_attr(feature = "async-trait", async_trait::async_trait)]
impl Actor for SessionWriter {
    type Msg = SessionWriterMessage;
    type Arguments = ActorWriteHalf;
    type State = SessionWriterState;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        writer: ActorWriteHalf,
    ) -> Result<Self::State, ActorProcessingErr> {
        // OK we've established connection, now we can process requests

        Ok(Self::State {
            writer: Some(writer),
        })
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // drop the channel to close it should we be exiting
        drop(state.writer.take());
        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SessionWriterMessage::Write(msg) if state.writer.is_some() => {
                if let Some(stream) = &mut state.writer {
                    if let ActorWriteHalf::Regular(w) = stream {
                        w.writable().await?;
                    }

                    if let Err(write_err) = stream.write_u64(msg.len() as u64).await {
                        tracing::warn!("Error writing to the stream '{}'", write_err);
                    } else {
                        tracing::trace!("Wrote length, writing payload (len={})", msg.len());
                        // now send the object
                        if let Err(write_err) = stream.write_all(&msg).await {
                            tracing::warn!("Error writing to the stream '{}'", write_err);
                            myself.stop(Some("channel_closed".to_string()));
                            return Ok(());
                        }
                        // flush the stream
                        stream.flush().await?;
                    }
                }
            }
            _ => {
                // no-op, wait for next send request
            }
        }
        Ok(())
    }
}

// =========== Tcp Session reader ============ //

struct SessionReader {
    session: ActorRef<TcpSessionMessage>,
}

/// The node connection messages
pub enum SessionReaderMessage {
    /// Wait for an object from the stream
    WaitForFrame,

    /// Read next object off the stream
    ReadFrame(u64),
}

struct SessionReaderState {
    reader: Option<ActorReadHalf>,
}

#[cfg_attr(feature = "async-trait", async_trait::async_trait)]
impl Actor for SessionReader {
    type Msg = SessionReaderMessage;
    type Arguments = ActorReadHalf;
    type State = SessionReaderState;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        reader: ActorReadHalf,
    ) -> Result<Self::State, ActorProcessingErr> {
        // start waiting for the first object on the network
        let _ = myself.cast(SessionReaderMessage::WaitForFrame);
        Ok(Self::State {
            reader: Some(reader),
        })
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // drop the channel to close it should we be exiting
        drop(state.reader.take());
        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            Self::Msg::WaitForFrame if state.reader.is_some() => {
                if let Some(stream) = &mut state.reader {
                    match stream.read_u64().await {
                        Ok(length) => {
                            tracing::trace!("Payload length message ({}) received", length);
                            let _ = myself.cast(SessionReaderMessage::ReadFrame(length));
                            return Ok(());
                        }
                        Err(err) if err.kind() == ErrorKind::UnexpectedEof => {
                            tracing::trace!("Error (EOF) on stream");
                            // EOF, close the stream by dropping the stream
                            drop(state.reader.take());
                            myself.stop(Some("channel_closed".to_string()));
                        }
                        Err(_other_err) => {
                            tracing::trace!("Error ({:?}) on stream", _other_err);
                            // some other TCP error, more handling necessary
                        }
                    }
                }

                let _ = myself.cast(SessionReaderMessage::WaitForFrame);
            }
            Self::Msg::ReadFrame(length) if state.reader.is_some() => {
                if let Some(stream) = &mut state.reader {
                    match read_n_bytes(stream, length as usize).await {
                        Ok(buf) => {
                            tracing::trace!("Payload of length({}) received", buf.len());
                            // NOTE: Our implementation writes 2 messages when sending something over the wire, the first
                            // is exactly 8 bytes which constitute the length of the payload message (u64 in big endian format),
                            // followed by the payload. This tells our TCP reader how much data to read off the wire

                            self.session.cast(TcpSessionMessage::FrameReady(buf))?;
                        }
                        Err(err) if err.kind() == ErrorKind::UnexpectedEof => {
                            // EOF, close the stream by dropping the stream
                            drop(state.reader.take());
                            myself.stop(Some("channel_closed".to_string()));
                            return Ok(());
                        }
                        Err(_other_err) => {
                            // TODO: some other TCP error, more handling necessary
                        }
                    }
                }

                // we've read the object, now wait for next object
                let _ = myself.cast(SessionReaderMessage::WaitForFrame);
            }
            _ => {
                // no stream is available, keep looping until one is available
                let _ = myself.cast(SessionReaderMessage::WaitForFrame);
            }
        }
        Ok(())
    }
}
