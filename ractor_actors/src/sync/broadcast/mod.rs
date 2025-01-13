// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

//! Broadcasting actor which is responsible for sending messages
//! to a collection of downstream targets.
//!
//! to be added: examples in docs

use std::collections::HashMap;
use std::marker::PhantomData;

use ractor::Actor;
use ractor::ActorId;
use ractor::ActorProcessingErr;
use ractor::ActorRef;
use ractor::RpcReplyPort;

#[cfg(test)]
mod tests;

/// The target of a broadcast operation. This is traditionally
/// a wrapper over the underlying [ActorRef] and is used for
/// message translation during the message forwarding process.
pub trait BroadcastTarget<T>: 'static + Send
where
    T: ractor::Message + Clone,
{
    /// Retrieve the id of this broadcast target, should be the
    /// underlying [ActorRef]'s id
    fn id(&self) -> ActorId;

    /// Send a message to the underlying [ActorRef]. Any necessary
    /// translation from `T` to the target's message type should
    /// be done here before forwarding. It's not an async translation
    /// since it should (a) be VERY fast and (b) not block the
    /// thread for long
    fn send(&self, t: T) -> Result<(), ActorProcessingErr>;
}

/// Represents an actor which broadcasts information to a colletion of downstream
/// actors, cloning and forwarding the message to all targets.
pub struct Broadcaster<T>
where
    T: ractor::Message + Clone,
{
    _t: PhantomData<T>,
}

impl<T> Default for Broadcaster<T>
where
    T: ractor::Message + Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Broadcaster<T>
where
    T: ractor::Message + Clone,
{
    /// Creates a new [Broadcaster] with the phantom data element populated.
    pub fn new() -> Self {
        Self { _t: PhantomData }
    }

    /// Builds a [BroadcastTarget] where no translation is necessary (i.e. the downstream
    /// actor receives handles the message type `T` directly without conversion)
    ///
    /// * `who` - The [ActorRef] to messages to
    ///
    /// Returns a boxed [BroadcastTarget].
    pub fn get_unit_mapped_target(who: ActorRef<T>) -> Box<dyn BroadcastTarget<T>>
    where
        T: ractor::Message + Clone,
    {
        struct IdTarget<T2>
        where
            T2: ractor::Message + Clone,
        {
            ar: ActorRef<T2>,
        }

        impl<T2> BroadcastTarget<T2> for IdTarget<T2>
        where
            T2: ractor::Message + Clone,
        {
            fn id(&self) -> ActorId {
                self.ar.get_id()
            }
            fn send(&self, t: T2) -> Result<(), ActorProcessingErr> {
                self.ar.cast(t)?;
                Ok(())
            }
        }

        Box::new(IdTarget::<T> { ar: who })
    }
}

/// Initial configuration for the [Broadcaster] actor
#[derive(Default)]
pub struct BroadcasterConfig<T>
where
    T: ractor::Message + Clone,
{
    /// (optional) Initial broadcast targets to send messages to
    pub initial_targets: Vec<Box<dyn BroadcastTarget<T>>>,
    /// If [true], this flag controls if targets which throw an
    /// error sending a message should be retried on future mesages.
    /// If [false], a downstream error will take down the [Broadcaster]
    /// actor, halting all future broadcasts.
    pub continue_with_dead_targets: bool,
}

// SAFETY: `T` is only present as PhantomData, so it doesn't actually affect if the
// [Broadcaster] is Sync across thread boundaries.
unsafe impl<T> Sync for Broadcaster<T> where T: ractor::Message + Clone {}

/// Messages supported by the [Broadcaster] actor
pub enum BroadcasterMessage<T>
where
    T: ractor::Message + Clone,
{
    /// Broadcast a message
    Broadcast(T),
    /// Add a new downstream target to the broadcast
    AddTarget(Box<dyn BroadcastTarget<T>>),
    /// Remove a target from the downstream targets for bradcast operations
    RemoveTarget(ActorId),
    /// Get the ids of the downstream broadcast targets
    ListTargets(RpcReplyPort<Vec<ActorId>>),
}

#[doc(hidden)]
pub struct BroadcasterState<T>
where
    T: ractor::Message + Clone,
{
    targets: HashMap<ActorId, Box<dyn BroadcastTarget<T>>>,
    continue_with_dead_targets: bool,
}

#[cfg_attr(feature = "async-trait", async_trait::async_trait)]
impl<T> Actor for Broadcaster<T>
where
    T: ractor::Message + Clone,
{
    type Msg = BroadcasterMessage<T>;
    type State = BroadcasterState<T>;
    type Arguments = BroadcasterConfig<T>;

    async fn pre_start(
        &self,
        _: ActorRef<Self::Msg>,
        BroadcasterConfig {
            continue_with_dead_targets,
            initial_targets,
        }: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(BroadcasterState {
            targets: initial_targets
                .into_iter()
                .map(|target| (target.id(), target))
                .collect(),
            continue_with_dead_targets,
        })
    }

    async fn handle(
        &self,
        _: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            BroadcasterMessage::Broadcast(t) => {
                for (who, target) in state.targets.iter() {
                    if let Err(e) = target.send(t.clone()) {
                        tracing::error!("Error forwarding message to target {who}: {e}");
                        if !state.continue_with_dead_targets {
                            // Fail the broadcaster actor
                            return Err(e);
                        }
                    } else {
                        tracing::debug!("Broadcast message to {who}");
                    }
                }
            }
            BroadcasterMessage::AddTarget(target) => {
                state.targets.insert(target.id(), target);
            }
            BroadcasterMessage::RemoveTarget(target) => {
                state.targets.remove(&target);
            }
            BroadcasterMessage::ListTargets(reply) => {
                let ids = state.targets.keys().cloned().collect::<Vec<_>>();
                let _ = reply.send(ids);
            }
        }
        Ok(())
    }
}
