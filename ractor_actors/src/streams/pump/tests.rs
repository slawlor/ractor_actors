// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

use ractor::call;
use ractor::concurrency::Duration;
use ractor::Actor;
use ractor::ActorProcessingErr;
use ractor::ActorRef;
use ractor::RpcReplyPort;
use tokio_stream as stream;

use crate::common_test::periodic_async_check;

use super::spawn_stream_pump;

struct StreamActor;

enum StreamActorMessage {
    GetCount(RpcReplyPort<u64>),
    Add(u64),
}

#[cfg_attr(feature = "async-trait", async_trait::async_trait)]
impl Actor for StreamActor {
    type Msg = StreamActorMessage;
    type State = u64;
    type Arguments = ();

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        _: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        // just for the test, drop the cell
        let _ = spawn_stream_pump(
            stream::iter(1u64..=500u64),
            myself.clone(),
            |a| {
                if a.is_some() {
                    StreamActorMessage::Add(1)
                } else {
                    StreamActorMessage::Add(0)
                }
            },
            None,
        )
        .await?;

        Ok(0)
    }

    async fn handle(
        &self,
        _: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            StreamActorMessage::GetCount(reply) => {
                println!("Received count request");
                let _ = reply.send(*state);
            }
            StreamActorMessage::Add(i) => {
                *state += i;
            }
        }
        Ok(())
    }

    async fn handle_supervisor_evt(
        &self,
        _myself: ActorRef<Self::Msg>,
        _message: ractor::SupervisionEvent,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // ignore child fails (stream exits)
        Ok(())
    }
}

#[ractor::concurrency::test]
#[tracing_test::traced_test]
async fn test_streaming_operation() {
    // Setup
    // Create the actor
    let (actor, handle) = Actor::spawn(None, StreamActor, ())
        .await
        .expect("Failed to spawn non-blocking actor tree");

    periodic_async_check(
        || async {
            // get the count
            let reply = call!(actor, StreamActorMessage::GetCount).expect("Failed to get count");
            reply >= 1
        },
        Duration::from_secs(3),
    )
    .await;

    // Cleanup
    actor.stop(None);
    handle.await.unwrap();
}
