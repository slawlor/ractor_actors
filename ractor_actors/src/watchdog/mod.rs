// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

//! Actor watchdogs
//!
//! An actor can use a watchdog to terminate itself if it's not feeding the watchdog. After an actor
//! is registered with the watchdog, it has to ping the watchdog within the set timeout.
//!
//! This can be used by actors that has certain deadlines, for example user interactivity.
//!
//! If an application has a per-user actor and is waiting for a user response within a certain time,
//! it can use this module to do so. The actor can either be stopped or killed.
//!
//! To use the watchdog, the application has to first call [register] and then call [ping] on a
//! regular interval. The watchdog can be disabled by calling [unregister], but its usage is
//! optional. If an actor terminates for any reason before the watchdog fires, it is simply cleaned
//! up.
//!
//! ```rust
//! use ractor::*;
//! use ractor::concurrency::Duration;
//! use ractor_actors::watchdog;
//! use ractor_actors::watchdog::TimeoutStrategy;
//! struct MyActor;
//!
//! enum MyActorMsg {
//!     UserInput(String),
//! }
//!
//! #[cfg_attr(feature = "async-trait", async_trait::async_trait)]
//! impl Actor for MyActor {
//!     type Msg = MyActorMsg;
//!     type State = ();
//!     type Arguments = ();
//!
//!     async fn pre_start(
//!         &self,
//!         myself: ActorRef<Self::Msg>,
//!         args: Self::Arguments,
//!     ) -> Result<Self::State, ActorProcessingErr> {
//!         // Register with the watchdog. If this actor don't ping it once every second, it will
//!         // be stopped.
//!         watchdog::register(
//!             myself.get_cell(),
//!             Duration::from_secs(1),
//!             TimeoutStrategy::Stop,
//!         )
//!         .await?;
//!
//!         Ok(())
//!     }
//!
//!     async fn post_stop(
//!         &self,
//!         _: ActorRef<Self::Msg>,
//!         _: &mut Self::State,
//!     ) -> Result<(), ActorProcessingErr> {
//!         println!("Input timeout!");
//!
//!         Ok(())
//!     }
//!     async fn handle(
//!         &self,
//!         myself: ActorRef<Self::Msg>,
//!         message: Self::Msg,
//!         state: &mut Self::State,
//!     ) -> Result<(), ActorProcessingErr> {
//!         match message {
//!             Self::Msg::UserInput(msg) => {
//!                 // When we get a message from the user, ping the watchdog
//!                 watchdog::ping(myself.get_id()).await?;
//!                 println!("User input: {}", msg);
//!             }
//!             // ... handle other messages
//!         }
//!
//!         Ok(())
//!     }
//! }
//! ```
//!
//! # Implementation note
//!
//! The watchdog is implemented as a single actor internally. This means that it uses the same,
//! global executors as the actors it is monitoring. If some actors are doing CPU bound work the
//! internal actor might be starved for CPU and not able to kill the monitored actors.
//!
//! Make sure that this fits your use-case.

use r#impl::WatchdogMsg;
use ractor::concurrency::Duration;
use ractor::rpc::CallResult;
use ractor::{Actor, ActorId, ActorRef, MessagingErr, RpcReplyPort};
use ractor::{ActorCell, ActorProcessingErr};
use tokio::sync::OnceCell;

/// See [register]. Controls what the watchdog will do on timeout.
pub enum TimeoutStrategy {
    /// This will call [ActorCell::kill].
    Kill,
    /// This will call [ActorCell::stop] with [WATCHDOG_TIMEOUT] as stop reason.
    Stop,
}

/// The stop reason that will be used when an actor is stopped by a watchdog timeout.
pub const WATCHDOG_TIMEOUT: &str = "watchdog_timeout";

/// Register an actor with the watchdog.
///
/// # Arguments
///
/// * `actor` - the actor that is to be watched.
/// * `duration` - the max duration between each ping.
/// * `timeout_strategy` - What the actor should do on a timeout. See [TimeoutStrategy].
///
pub async fn register(
    actor: ActorCell,
    duration: Duration,
    timeout_strategy: TimeoutStrategy,
) -> Result<(), MessagingErr<()>> {
    cast(WatchdogMsg::Register(actor, duration, timeout_strategy)).await
}

/// Unregister an actor from the watchdog.
///
/// # Arguments
///
/// * `actor` - the actor to unregister.
pub async fn unregister(actor: ActorCell) -> Result<(), MessagingErr<()>> {
    cast(WatchdogMsg::Unregister(actor)).await
}

/// Send a ping to the watchdog. Doing this within the timeout prevents the watchdog from
/// terminating the actor.
///
/// # Arguments
///
/// * `actor` - the actor that is sending the ping.
pub async fn ping(actor: ActorId) -> Result<(), MessagingErr<()>> {
    call(|reply| WatchdogMsg::Ping(actor, reply)).await
}

/// The return value from [stats] that describes
pub struct WatchdogStats {
    pub kills: usize,
}

pub async fn stats() -> Result<WatchdogStats, MessagingErr<()>> {
    call(WatchdogMsg::Stats).await
}

static WATCHDOG: OnceCell<Result<ActorRef<WatchdogMsg>, ActorProcessingErr>> =
    OnceCell::const_new();

async fn spawn() -> Result<ActorRef<WatchdogMsg>, ActorProcessingErr> {
    let (watchdog, _) = Actor::spawn(None, r#impl::Watchdog {}, ()).await?;

    Ok(watchdog)
}

async fn cast(msg: WatchdogMsg) -> Result<(), MessagingErr<()>> {
    match WATCHDOG.get_or_init(spawn).await {
        Ok(watchdog) => watchdog.cast(msg).map_err(|_| MessagingErr::SendErr(())),
        Err(_) => Err(MessagingErr::SendErr(())),
    }
}

async fn call<TReply, TMsgBuilder>(msg_builder: TMsgBuilder) -> Result<TReply, MessagingErr<()>>
where
    TMsgBuilder: FnOnce(RpcReplyPort<TReply>) -> WatchdogMsg,
{
    match WATCHDOG.get_or_init(spawn).await {
        Ok(watchdog) => match watchdog
            .call(msg_builder, None)
            .await
            .map_err(|_| MessagingErr::SendErr(()))?
        {
            CallResult::Success(result) => Ok(result),
            _ => Err(MessagingErr::SendErr(())),
        },
        Err(_) => Err(MessagingErr::SendErr(())),
    }
}

mod r#impl;

#[cfg(test)]
pub mod tests;
