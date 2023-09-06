// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

//! Streaming utilities built off of [ractor] actors. This includes building a looped
//! operation (see [looping]) and actors which process streams (see: [pump] and [mux]).

pub mod looping;
pub mod mux;
pub mod pump;

// Re-exports
pub use looping::spawn_loop;
pub use looping::IterationResult;
pub use looping::Operation;
pub use mux::mux_stream;
pub use mux::StreamMuxConfiguration;
pub use mux::StreamMuxNotification;
pub use mux::Target;
pub use pump::spawn_stream_pump;
