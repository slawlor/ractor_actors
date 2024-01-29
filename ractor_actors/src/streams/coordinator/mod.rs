// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

#![allow(missing_docs)]

//! ## Stream coordination
//!
//! Streams often should be treated as a first-class citizen, meaning
//! that the items in the stream aren't necessarily dequeued sequentially
//! one-by-one locally, and some higher-order processing may occur (i.e.
//! batches of items or something.).
//!
//! That being said, we still might want to stop/start processing the stream
//! via a 1-to-1 supervisory process where the child process blocks on a single
//! streaming operation and the supervisor/parent can receive external operational
//! requests.

use std::marker::PhantomData;

use ractor::{Actor, ActorProcessingErr, ActorRef};
use tokio_stream::Stream;

// TODO: fill out stream operation logic
#[ractor::async_trait]
pub trait StreamHandler<S>: ractor::State 
where
    S: Stream + ractor::State,
{
    /// Process the stream with a singular operation
    async fn process(&mut self, stream: S);
}

pub struct StreamOperator<S, H>
where
    S: Stream + ractor::State,
    H: StreamHandler<S>,
{
    _s: PhantomData<S>,
    _h: PhantomData<H>,
}

impl<S, H> StreamOperator<S, H>
where
    S: Stream + ractor::State,
    S::Item: Clone + ractor::Message,
    H: StreamHandler<S>,
{
    /// Create a new [StreamOperator] instance
    pub fn new() -> Self {
        Self {
            _s: PhantomData,
            _h: PhantomData,
        }
    }
}

pub enum StreamOperation {
    /// Pause processing the stream (will complete next item)
    Pause,
    /// Stop processing the stream completely (will terminate and drop the stream)
    Stop,
    /// Continue processing the stream if paused
    Continue,
}

// SAFETY: Since the struct only contains phantom data, it is completely fine to
// have non sync-safe types internally in the struct, as it won't actually cross
// any thread boundary.
unsafe impl<S, H> Sync for StreamOperator<S, H>
where
    S: Stream + ractor::State,
    H: StreamHandler<S>,
{
}

/// State of the [StreamOperator] actor
pub struct StreamOperatorState {
    #[allow(unused)]
    child: ActorRef<()>,
}

#[ractor::async_trait]
impl<S, H> Actor for StreamOperator<S, H>
where
    S: Stream + ractor::State,
    H: StreamHandler<S>,
{
    // TODO: make a struct type for this
    type Arguments = (S, H);
    type State = StreamOperatorState;
    type Msg = StreamOperation;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (child, _) = Actor::spawn_linked(
            None,
            internal::InternalStreamOperator::<S, H>::new(),
            args,
            myself.into(),
        )
        .await?;
        Ok(StreamOperatorState { child })
    }
}

mod internal {
    use super::*;

    pub struct InternalStreamOperator<S, H>
    where
        S: Stream + ractor::State,
        H: StreamHandler<S>,
    {
        _s: PhantomData<S>,
        _h: PhantomData<H>,
    }

    impl<S, H> InternalStreamOperator<S, H>
    where
        S: Stream + ractor::State,
        H: StreamHandler<S>,
    {
        pub fn new() -> Self {
            Self {
                _s: PhantomData,
                _h: PhantomData,
            }
        }
    }

    // SAFETY: Since the struct only contains phantom data, it is completely fine to
    // have non sync-safe types internally in the struct, as it won't actually cross
    // any thread boundary.
    unsafe impl<S, H> Sync for InternalStreamOperator<S, H>
    where
        S: Stream + ractor::State,
        H: StreamHandler<S>,
    {
    }

    pub struct InternalStreamOperatorState<S, H>
    where
        S: Stream + ractor::State,
        H: StreamHandler<S>,
    {
        stream: S,
        handler: H,
    }

    #[ractor::async_trait]
    impl<S, H> Actor for InternalStreamOperator<S, H>
    where
        S: Stream + ractor::State,
        H: StreamHandler<S>,
    {
        type Arguments = (S, H);
        type State = InternalStreamOperatorState<S, H>;
        type Msg = ();

        async fn pre_start(
            &self,
            _myself: ActorRef<Self::Msg>,
            (stream, handler): Self::Arguments,
        ) -> Result<Self::State, ActorProcessingErr> {
            Ok(InternalStreamOperatorState { stream, handler })
        }
    }
}
