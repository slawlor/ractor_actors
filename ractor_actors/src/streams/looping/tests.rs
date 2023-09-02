// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

use ractor::call;
use ractor::concurrency::sleep;
use ractor::concurrency::Duration;
use ractor::RpcReplyPort;

use super::*;

struct BackgroundAdder;

#[ractor::async_trait]
impl Operation for BackgroundAdder {
    type State = ActorRef<TestBedMessage>;

    async fn work(&self, state: &mut Self::State) -> Result<IterationResult, ActorProcessingErr> {
        cast!(state, TestBedMessage::Add(1))?;
        sleep(Duration::from_millis(25)).await;
        Ok(IterationResult::Continue)
    }
}

struct TestBedActor;

enum TestBedMessage {
    GetCount(RpcReplyPort<u64>),
    Add(u64),
}

#[ractor::async_trait]
impl Actor for TestBedActor {
    type Msg = TestBedMessage;
    type State = u64;
    type Arguments = ();

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        _: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        // just for the test, drop the cell
        let _ = super::spawn_loop(BackgroundAdder, myself.clone(), Some(myself.get_cell())).await?;

        Ok(0)
    }

    async fn handle(
        &self,
        _: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            TestBedMessage::GetCount(reply) => {
                let _ = reply.send(*state);
            }
            TestBedMessage::Add(i) => {
                *state += i;
            }
        }
        Ok(())
    }
}

#[ractor::concurrency::test]
async fn test_looping_operation() {
    // Setup
    // Create the actor
    let (actor, handle) = Actor::spawn(None, TestBedActor, ())
        .await
        .expect("Failed to spawn non-blocking actor tree");

    // Allow the background blocking operation some time to increment the parent's counter
    sleep(Duration::from_millis(100)).await;

    // Assert

    // Get the count
    let reply = call!(actor, TestBedMessage::GetCount).expect("Failed to get count");
    assert!(reply >= 3);

    // Cleanup
    actor.stop(None);
    handle.await.unwrap();
}
