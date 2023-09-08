// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

//! Synchronization primative actors. Contained is
//!
//! * [broadcast::Broadcaster] - a broadcast actor which will take inputs
//! and forward them to downstream targets.

pub mod broadcast;
