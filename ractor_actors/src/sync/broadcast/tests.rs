// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use ractor::concurrency::Duration;
use ractor::ActorStatus;

use crate::common_test::periodic_check;

use super::*;

type TestMessage = u32;

struct HappyActor;
#[cfg_attr(feature = "async-trait", async_trait::async_trait)]
impl Actor for HappyActor {
    type Msg = TestMessage;
    type State = Arc<AtomicU32>;
    type Arguments = Arc<AtomicU32>;

    async fn pre_start(
        &self,
        _: ActorRef<Self::Msg>,
        start: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(start)
    }

    async fn handle(
        &self,
        _: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        state.fetch_add(message, Ordering::Relaxed);
        Ok(())
    }
}

struct BoomActor;
#[cfg_attr(feature = "async-trait", async_trait::async_trait)]
impl Actor for BoomActor {
    type Msg = TestMessage;
    type State = Arc<AtomicU32>;
    type Arguments = Arc<AtomicU32>;

    async fn pre_start(
        &self,
        _: ActorRef<Self::Msg>,
        start: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(start)
    }

    async fn handle(
        &self,
        _: ActorRef<Self::Msg>,
        _: Self::Msg,
        _: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        Err(From::from("boom!"))
    }
}

#[ractor::concurrency::test]
#[tracing_test::traced_test]
async fn test_broadcast() {
    // setup
    let counter = Arc::new(AtomicU32::new(0));
    let mut actors = vec![];
    for _ in 0..3 {
        actors.push(
            Actor::spawn(None, HappyActor, counter.clone())
                .await
                .expect("Failed to start test actor"),
        );
    }
    let initial_targets = actors
        .iter()
        .map(|(a, _)| a.clone())
        .map(Broadcaster::get_unit_mapped_target)
        .collect::<Vec<_>>();
    let config = BroadcasterConfig {
        continue_with_dead_targets: false,
        initial_targets,
    };
    let (bactor, bhandle) = Actor::spawn(None, Broadcaster::new(), config)
        .await
        .expect("Failed to start broadcast actor");

    // act
    for _ in 0..3 {
        bactor
            .cast(BroadcasterMessage::Broadcast(12))
            .expect("Failed to broadcast");
    }

    periodic_check(
        || {
            let expected: u32 = 3 * 3 * 12;
            expected == counter.load(Ordering::Relaxed)
        },
        Duration::from_secs(1),
    )
    .await;

    // cleanup
    bactor.stop(None);
    bhandle.await.unwrap();
    // cleanup targets
    for (actor, _) in actors.iter() {
        actor.stop(None);
    }
    for (_, handle) in actors.into_iter() {
        handle.await.unwrap();
    }
}

#[ractor::concurrency::test]
#[tracing_test::traced_test]
async fn test_broadcast_bad_targets() {
    // setup
    let counter = Arc::new(AtomicU32::new(0));
    let mut actors = vec![];
    for i in 0..3 {
        if i == 0 {
            actors.push(
                Actor::spawn(None, HappyActor, counter.clone())
                    .await
                    .expect("Failed to start test actor"),
            );
        } else {
            actors.push(
                Actor::spawn(None, BoomActor, counter.clone())
                    .await
                    .expect("Failed to start test actor"),
            );
        }
    }
    let initial_targets = actors
        .iter()
        .map(|(a, _)| a.clone())
        .map(Broadcaster::get_unit_mapped_target)
        .collect::<Vec<_>>();
    let config = BroadcasterConfig {
        continue_with_dead_targets: true,
        initial_targets,
    };
    let (bactor, bhandle) = Actor::spawn(None, Broadcaster::new(), config)
        .await
        .expect("Failed to start broadcast actor");

    // act
    for _ in 0..3 {
        bactor
            .cast(BroadcasterMessage::Broadcast(12))
            .expect("Failed to broadcast");
    }

    periodic_check(
        || {
            // only 1 actor processes messages, and got 3 messages of "12"
            let expected: u32 = 3 * 12;
            expected == counter.load(Ordering::Relaxed)
        },
        Duration::from_secs(1),
    )
    .await;

    // cleanup
    bactor.stop(None);
    bhandle.await.unwrap();
    // cleanup targets
    for (actor, _) in actors.iter() {
        actor.stop(None);
    }
    for (_, handle) in actors.into_iter() {
        handle.await.unwrap();
    }
}

#[ractor::concurrency::test]
#[tracing_test::traced_test]
async fn test_broadcast_bad_targets_fails_on_target_failure() {
    // setup
    let counter = Arc::new(AtomicU32::new(0));
    let mut actors = vec![];
    for i in 0..3 {
        if i == 0 {
            actors.push(
                Actor::spawn(None, HappyActor, counter.clone())
                    .await
                    .expect("Failed to start test actor"),
            );
        } else {
            actors.push(
                Actor::spawn(None, BoomActor, counter.clone())
                    .await
                    .expect("Failed to start test actor"),
            );
        }
    }
    let initial_targets = actors
        .iter()
        .map(|(a, _)| a.clone())
        .map(Broadcaster::get_unit_mapped_target)
        .collect::<Vec<_>>();
    let config: BroadcasterConfig<u32> = BroadcasterConfig {
        continue_with_dead_targets: false,
        initial_targets,
    };
    let (bactor, bhandle) = Actor::spawn(None, Broadcaster::new(), config)
        .await
        .expect("Failed to start broadcast actor");

    // act
    periodic_check(
        || {
            // After the downstream actors fails processing a message, subsequent cast operations will "fail"
            let _ = bactor.cast(BroadcasterMessage::Broadcast(12));
            bactor.get_status() == ActorStatus::Stopped
        },
        Duration::from_secs(1),
    )
    .await;

    // cleanup
    bhandle.await.unwrap();
    // cleanup targets
    for (actor, _) in actors.iter() {
        actor.stop(None);
    }
    for (_, handle) in actors.into_iter() {
        handle.await.unwrap();
    }
}
