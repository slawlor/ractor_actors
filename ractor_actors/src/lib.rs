// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

//! Helpful utility actors built on top of [ractor].
//!
//! ## What actors are available?
//!
//! There are multiple actors and more will follow as time progresses, however at the time
//! of this writing this crate includes
//!
//! 1. Basic TCP functionality ([net])
//! 2. Filewatcher ([filewatcher])
//! 3. Cron job management ([time])
//! 4. Stream processing (infinite/finite loop/stream processing, stream multiplexing, etc) ([streams])
//!
//! NOTE: This crate is still a work-in-progress and more functionality will be
//! added as time progresses
//!
//! ## Crate organization
//!
//! The crate is organized into sub-modules gated with features. This way you
//! can include as much or as little of the functionality that you might want.
//!
//! Each module should be self-documenting, and this root lib will likely contain little
//! information in favor of module-specific documentation.
//!
//! ## Installation
//!
//! ```toml
//! [dependencies]
//! ractor_actors = "0.1"
//! ```
//!

#![deny(missing_docs)]
#![deny(warnings)]

#[cfg(feature = "filewatcher")]
pub mod filewatcher;

#[cfg(feature = "net")]
pub mod net;

#[cfg(feature = "time")]
pub mod time;

#[cfg(feature = "streams")]
pub mod streams;

#[cfg(feature = "sync")]
pub mod sync;

#[cfg(test)]
pub(crate) mod common_test;
