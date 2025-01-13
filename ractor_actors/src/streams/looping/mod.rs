// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

//! An actor which wraps an operation that should be done in a looped fashion.
//! This actor continually executes the operation in an infinite basis until
//! the operation either signals it's complete or throws an error.
//!
//! To keep dependent services agnostic to the underlying actor logic, you can simply
//! use [spawn_loop] to start an operation which should be done in a loop.
//!
//! ```
//! use ractor::ActorProcessingErr;
//! use ractor::concurrency::Duration;
//! use ractor::concurrency::sleep;
//! use ractor_actors::streams::looping::{Operation, IterationResult, spawn_loop};
//!
//! struct SampleOperation;
//!
//! #[async_trait::async_trait]
//! impl Operation for SampleOperation {
//!     type State = ();
//!
//!     async fn work(&self, state: &mut Self::State) -> Result<IterationResult, ActorProcessingErr> {
//!         println!("I'm a loop!");
//!         sleep(Duration::from_millis(25)).await;
//!         Ok(IterationResult::Continue)
//!     }
//! }
//! let _ = async {
//!     let _actor = spawn_loop(SampleOperation, (), None).await.expect("Failed to start sample");
//! };
//! ```

use std::marker::PhantomData;

use ractor::cast;
use ractor::Actor;
use ractor::ActorCell;
use ractor::ActorProcessingErr;
use ractor::ActorRef;
use ractor::SpawnErr;

#[cfg(test)]
mod tests;

/// Represents the result of a looping operation. It signals if the
/// loop operation is should continue or end.
pub enum IterationResult {
    /// There's more to process, keep looping
    Continue,
    /// We're reached the end of the loop, stop processing
    End,
}

/// An operation is an implementation which is repeatedly called, blocking the task, until
/// either (a) shutdown ([IterationResult::End]) or (b) error. This could be processing a stream
/// request or some continuous polling operation (something like a configuration element,
/// which emits signals when the config changes)
#[async_trait::async_trait]
pub trait Operation: ractor::State + Sync {
    /// The state of the looping operation. It is bound to the internal actor's [Actor::State]
    /// but doesn't require specifying all of the inner actor properties
    type State: ractor::State;

    /// Do this loop's worth of work. Returning a signal if there's more loop processing to do
    /// or we should stop processing
    async fn work(&self, state: &mut Self::State) -> Result<IterationResult, ActorProcessingErr>;
}

/// A blocking actor which performs a given async operation continually
/// in a loop and does not handle external messages. It is strongly typed
/// to the inner implementation of the [Operation] that it's looping on.
struct Loop<T>
where
    T: Operation,
{
    _t: PhantomData<fn() -> T>,
}

#[cfg_attr(feature = "async-trait", async_trait::async_trait)]
impl<TOperation> Actor for Loop<TOperation>
where
    TOperation: Operation,
{
    type Msg = ();
    type State = (TOperation, TOperation::State);
    type Arguments = (TOperation, TOperation::State);

    async fn pre_start(
        &self,
        myself: ActorRef<()>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        // start the loop operation
        cast!(myself, ())?;
        Ok(args)
    }

    async fn handle(
        &self,
        myself: ActorRef<()>,
        _: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // perform the operation
        let result = state.0.work(&mut state.1).await?;
        if let IterationResult::Continue = result {
            cast!(myself, ())?;
        } else {
            myself.stop(None);
        }
        Ok(())
    }
}

/// Spawn an operation which will continue executing until either
///
/// 1. One of the operation calls panics or returns an error
/// 2. The operation signals it's completed with [IterationResult::End]
///
/// The loop operation additionally can thread a state which can be used in loop operations.
///
/// * `op`: The [Operation] implementation defining the work at each loop iteration
/// * `istate`: The initial state for the loop operation
/// * `supervisor`: (Optional) The [ActorCell] supervisor actor which will supervise the underlying loop operation actor
///
/// Returns [Ok(ActorCell)] if the underlying actor was successfully spawned, [SpawnErr] if a startup failure occurs.
pub async fn spawn_loop<TOperation>(
    op: TOperation,
    istate: TOperation::State,
    supervisor: Option<ActorCell>,
) -> Result<ActorCell, SpawnErr>
where
    TOperation: Operation,
{
    let actor = Loop { _t: PhantomData };
    let actor_ref = if let Some(sup) = supervisor {
        Actor::spawn_linked(None, actor, (op, istate), sup).await?.0
    } else {
        Actor::spawn(None, actor, (op, istate)).await?.0
    };
    Ok(actor_ref.into())
}
