// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

//! This module provides two actors to manage TCP sessions with and without TLS encryption.
//! We utilize [tokio_rustls] for managing encrypted sessions based on `rustls`. There basic primative
//! actors supported here are
//!
//! 1. `TcpListener` - A server socket listener which accepts incoming requests and calls a callback on new received sessions
//! 2. [TcpSession] - An actor which manages reading and writing frames to a given socket. Can read and write concurrently through
//!    an interface actor
//! 3. [NetworkStream] - Represents either encrypted server or client sockets or an unencrypted socket.
//! 4. [FrameReceiver] - An implementation of this trait must be provided to the [TcpSession] in order to handle/decode incoming frames of
//!    messages.

pub mod listener;
pub mod session;

use std::net::SocketAddr;

use ractor::ActorProcessingErr;
use tokio::net::TcpStream;

/// A frame of data
pub type Frame = Vec<u8>;

/// A network port
pub type NetworkPort = u16;

/// A network data stream which can either be
/// 1. unencrypted
/// 2. encrypted and the server-side of the session
/// 3. encrypted and the client-side of the session
pub enum NetworkStream {
    /// Unencrypted session
    Raw {
        /// The peer's address
        peer_addr: SocketAddr,
        /// The local address
        local_addr: SocketAddr,
        /// The stream
        stream: TcpStream,
    },
    /// Encrypted as the server-side of the session
    TlsServer {
        /// The peer's address
        peer_addr: SocketAddr,
        /// The local address
        local_addr: SocketAddr,
        /// The stream
        stream: tokio_rustls::server::TlsStream<TcpStream>,
    },
    /// Encrypted as the client-side of the session
    TlsClient {
        /// The peer's address
        peer_addr: SocketAddr,
        /// The local address
        local_addr: SocketAddr,
        /// The stream
        stream: tokio_rustls::client::TlsStream<TcpStream>,
    },
}

impl NetworkStream {
    /// Retrieve the peer (other) socket address
    pub fn peer_addr(&self) -> SocketAddr {
        match self {
            Self::Raw { peer_addr, .. } => *peer_addr,
            Self::TlsServer { peer_addr, .. } => *peer_addr,
            Self::TlsClient { peer_addr, .. } => *peer_addr,
        }
    }

    /// Retrieve the local socket address
    pub fn local_addr(&self) -> SocketAddr {
        match self {
            Self::Raw { local_addr, .. } => *local_addr,
            Self::TlsServer { local_addr, .. } => *local_addr,
            Self::TlsClient { local_addr, .. } => *local_addr,
        }
    }
}

/// Incoming encryption mode
#[derive(Clone)]
pub enum IncomingEncryptionMode {
    /// Accept sockets raw, with no encryption
    Raw,
    /// Accept sockets and establish a secure connection
    Tls(tokio_rustls::TlsAcceptor),
}

/// Represents a receiver of frames of data
#[ractor::async_trait]
pub trait FrameReceiver: Send + Sync + 'static {
    /// Called when a frame is received by the [TcpSession] actor
    async fn frame_ready(&self, f: Frame) -> Result<(), ActorProcessingErr>;
}

/// Represents
#[ractor::async_trait]
pub trait SessionAcceptor: Send + Sync + 'static {
    /// Called when a new incoming session is received by a `TcpListener` actor
    async fn new_session(&self, session: NetworkStream) -> Result<(), ActorProcessingErr>;
}

pub use listener::*;
pub use session::*;
