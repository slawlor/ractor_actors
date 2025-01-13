// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

//! ## Processing a stream
//!
//! If you want to process a stream which emits records that should
//! be forwarded to an actor for processing, you can utilize [spawn_stream_pump]
//! to create an actor which will specifically process a stream of messages and
//! convert them to messages for an actor, pushing them into the actor's message queue.
//!
//! We specifically don't expose the internals of how the stream pump actor is created,
//! but still support monitoring the actor by returning an [ActorCell] which represents
//! the underlying actor, and can be used for pattern matching on supervision events.

use std::marker::PhantomData;
use std::pin::Pin;

use ractor::cast;
use ractor::ActorCell;
use ractor::ActorProcessingErr;
use ractor::ActorRef;
use ractor::SpawnErr;
use tokio_stream::Stream;
use tokio_stream::StreamExt;

use crate::streams::{spawn_loop, IterationResult, Operation};

#[cfg(test)]
mod tests;

struct StreamerState<S, T, F>
where
    S: Stream + ractor::State,
    T: ractor::Message,
    F: Fn(Option<<S as Stream>::Item>) -> T + ractor::State,
{
    stream: Pin<Box<S>>,
    fn_map: F,
    who: ActorRef<T>,
}

struct Streamer<S, T, F>
where
    S: Stream + ractor::State,
    T: ractor::Message,
    F: Fn(Option<<S as Stream>::Item>) -> T + ractor::State,
{
    _s: PhantomData<S>,
    _t: PhantomData<T>,
    _f: PhantomData<F>,
}

// SAFETY: The types here are all phantom data markers, and do not impact
// the requirement for streamer to be Sync
unsafe impl<S, T, F> Sync for Streamer<S, T, F>
where
    S: Stream + ractor::State,
    T: ractor::Message,
    F: Fn(Option<<S as Stream>::Item>) -> T + ractor::State,
{
}

#[async_trait::async_trait]
impl<S, T, F> Operation for Streamer<S, T, F>
where
    S: Stream + ractor::State,
    T: ractor::Message,
    F: Fn(Option<<S as Stream>::Item>) -> T + ractor::State,
{
    type State = StreamerState<S, T, F>;

    async fn work(&self, state: &mut Self::State) -> Result<IterationResult, ActorProcessingErr> {
        // fetch the next item and check if it's the last item
        let item = state.stream.next().await;
        let last = item.is_none();

        tracing::trace!("Streamer forwarding item: last {last}");

        cast!(state.who, (state.fn_map)(item))?;

        let signal = if last {
            IterationResult::End
        } else {
            IterationResult::Continue
        };
        Ok(signal)
    }
}

/// Process a stream of information, pumping messages received to a receipient
///
/// * `stream`: The stream of `Item`s to process
/// * `receiver`: The actor who will receive outputs from the stream
/// * `fn_map`: Mapping function to convert a `S::Item` to the input message for the recipient actor.
///   In the event the stream type is a sub-value of the message enum of the actor, this can be used
///   to map to the right sub-message type
/// * `supervisor`: (Optional) If the receiver is **not** the supervisor, a separate supervisor can be
///   provided here
///
/// Returns the [ActorCell] for the underlying stream pump actor upon successful startup, or a [SpawnErr] if the
/// underlying actor failed to spawn.
pub async fn spawn_stream_pump<S, T, F>(
    stream: S,
    receiver: ActorRef<T>,
    fn_map: F,
    supervisor: Option<ActorCell>,
) -> Result<ActorCell, SpawnErr>
where
    S: Stream + ractor::State,
    T: ractor::Message,
    F: Fn(Option<<S as Stream>::Item>) -> T + ractor::State,
{
    let sup = supervisor.unwrap_or_else(|| receiver.get_cell());

    let pumper = Streamer::<S, T, F> {
        _s: PhantomData,
        _t: PhantomData,
        _f: PhantomData,
    };
    let pump_state = StreamerState::<S, T, F> {
        fn_map,
        who: receiver,
        stream: Box::pin(stream),
    };

    spawn_loop(pumper, pump_state, Some(sup)).await
}
