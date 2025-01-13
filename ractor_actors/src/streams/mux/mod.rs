// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

//! ## Stream multiplexing
//!
//! Multiplexing a stream to a collection of targets is a common "broadcast"
//! operation. This actor supports taking an input stream and multiplexing the
//! data on the stream to multiple targets, configured at runtime.
//!
//! The multiplexer
//! 1. Takes a stream as input
//! 2. Takes a series of targets as inputs as well
//! 3. Use the stream pump to convert the stream to actor messages
//! 4. If a target fails, it will remove the target, notify of the failure,
//!    and continue processing the stream for the other targets
//! 5. Upon EOF for the stream, kill the actor cleanly, and cleanup downstream targets
//!

use std::{collections::HashSet, marker::PhantomData};

use ractor::{Actor, ActorCell, ActorProcessingErr, ActorRef, SpawnErr, SupervisionEvent};
use tokio_stream::Stream;

#[cfg(test)]
mod tests;

/// A callback which occurs when a downstream target cannot handle
/// a stream item.
pub trait StreamMuxNotification: 'static + Send {
    /// Called when a target fails to process an item with an error.
    ///
    /// If this callback occurs for a target, the target will **NOT**
    /// receive future items on the stream (we won't continue trying) if the
    /// `stop_processing_target_on_failure` flag is [true]
    fn target_failed(&self, target: String, err: ActorProcessingErr);

    /// Called when the stream terminates, either from reacing the end
    /// of the stream, or all targets failing and there being no more
    /// downstream targets to send stream items to.
    ///
    /// The stream actor will also exit in the supervision flow, but if
    /// you don't want to go through the supervision flow you can use
    /// this to notify when the stream ends.
    fn end_of_stream(&self);
}

/// A "target" of a stream's items. You can supply as many [Target]s
/// as you wish for a given stream, and for each item received, each
/// [Target] will receive a copy.
pub trait Target<S>: 'static + Send
where
    S: Stream + ractor::State,
    S::Item: Clone + ractor::Message,
{
    /// A unique id for this target, used for reporting. If it's an actor, this
    /// can be the [ractor::ActorId] serialized to a string which is guaranteed to be
    /// unique.
    fn get_id(&self) -> String;
    /// Called when an item is received. This is where downstream forwarding/handling may occur.
    fn message_received(&self, item: <S as Stream>::Item) -> Result<(), ActorProcessingErr>;
}

/// Configuration for a stream multiplexing. This signifies the stream, where to send it,
/// an informational callback, as well as processing control logic.
pub struct StreamMuxConfiguration<S, N>
where
    S: Stream + ractor::State,
    S::Item: Clone + ractor::Message,
    N: StreamMuxNotification,
{
    /// The stream to read items from
    pub stream: S,
    /// The collection of targets to forward the stream items to
    pub targets: Vec<Box<dyn Target<S>>>,
    /// The "informational callback" which is called when either a target
    /// fails or the stream finishes. See [StreamMuxNotification] for more information
    pub callback: N,
    /// If [true], this signifies that we should stop processing future items for a given
    /// [Target] upon the target returning an error handling an item. This can be used to
    /// avoid [Target]s which are broken or continually having problems processing items.
    pub stop_processing_target_on_failure: bool,
}

/// Mutiplex a stream to a collection of targets. Configured with a [StreamMuxConfiguration]
/// which controlls the entire setting.
///
/// * `config` - The stream mutiplex configuration. See [StreamMuxConfiguration] for the full
///   collection of arguments
/// * `sup` - (Optional) The supervisor for the underlying actor. If you wish to use the supervision
///   flow, this will connect the supervision flow for you
///
/// Returns [Ok(ActorCell)] signifying the underlying actor instance that was started upon successful
/// start, [Err(SpawnErr)] if the actor fails to start (or the inner stream-pump fails to start).
pub async fn mux_stream<S, N>(
    config: StreamMuxConfiguration<S, N>,
    sup: Option<ActorCell>,
) -> Result<ActorCell, SpawnErr>
where
    S: Stream + ractor::State,
    S::Item: Clone + ractor::Message,
    N: StreamMuxNotification,
{
    let handler = MuxActor::<S, N> {
        _s: PhantomData,
        _n: PhantomData,
    };
    let actor = if let Some(s) = sup {
        Actor::spawn_linked(None, handler, config, s).await?.0
    } else {
        Actor::spawn(None, handler, config).await?.0
    };

    Ok(actor.into())
}

// --------------------------- Actor Implementation --------------------------- //
struct MuxActorState<S, N>
where
    S: Stream + ractor::State,
    S::Item: Clone + ractor::Message,
    N: StreamMuxNotification,
{
    targets: Vec<Box<dyn Target<S>>>,
    callback: N,
    stop_processing_target_on_failure: bool,
    _pump: ActorCell,
}

/// Mux Actor
struct MuxActor<S, N>
where
    S: Stream + ractor::State,
    S::Item: Clone + ractor::Message,
    N: StreamMuxNotification,
{
    _s: PhantomData<S>,
    _n: PhantomData<N>,
}

// SAFETY: The types here are all phantom data markers, and do not impact
// the requirement for streamer to be Sync
unsafe impl<S, N> Sync for MuxActor<S, N>
where
    S: Stream + ractor::State,
    S::Item: Clone + ractor::Message,
    N: StreamMuxNotification,
{
}

#[cfg_attr(feature = "async-trait", async_trait::async_trait)]
impl<S, N> Actor for MuxActor<S, N>
where
    S: Stream + ractor::State,
    S::Item: Clone + ractor::Message,
    N: StreamMuxNotification,
{
    type Msg = Option<S::Item>;
    type State = MuxActorState<S, N>;
    type Arguments = StreamMuxConfiguration<S, N>;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        StreamMuxConfiguration::<S, N> {
            stream,
            targets,
            callback,
            stop_processing_target_on_failure,
        }: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let pump = crate::streams::spawn_stream_pump(stream, myself, |a| a, None).await?;
        tracing::debug!("Stream pump started");
        Ok(MuxActorState::<S, N> {
            _pump: pump,
            callback,
            targets,
            stop_processing_target_on_failure,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Option<S::Item>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        if let Some(item) = message {
            let mut to_be_removed = HashSet::new();
            for target in state.targets.iter() {
                if let Err(err) = target.message_received(item.clone()) {
                    let id = target.get_id();
                    tracing::error!("Failed to send message to target {} with {err}", id);
                    state.callback.target_failed(id.clone(), err);
                    to_be_removed.insert(id);
                }
                // else successfully sent notification
            }

            if state.stop_processing_target_on_failure {
                state
                    .targets
                    .retain(|target| !to_be_removed.contains(&target.get_id()));

                if state.targets.is_empty() {
                    tracing::debug!("Halting stream processing as no more targets exist");
                    myself.stop(None);
                    state.callback.end_of_stream();
                }
            }
        } else {
            myself.stop(Some("End of stream".to_string()));
            state.callback.end_of_stream();
            tracing::debug!("Reached end of stream");
        }
        Ok(())
    }

    async fn handle_supervisor_evt(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: SupervisionEvent,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        if let SupervisionEvent::ActorFailed(_who, what) = message {
            // bubble up panics, but not child exits (we may still be processing when the child stream pump stops)
            return Err(ractor::ActorErr::Failed(what).into());
        }
        Ok(())
    }
}
