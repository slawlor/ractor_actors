use super::WatchdogMsg::{Register, Stats};
use super::{TimeoutStrategy, WatchdogMsg};
use crate::watchdog::r#impl::Watchdog;
use ractor::*;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;
use std::time::Duration;
use tracing::info;

#[concurrency::test]
#[tracing_test::traced_test]
async fn test_foo() {
    static HANDLE: AtomicBool = AtomicBool::new(false);
    static POST_STOP: AtomicBool = AtomicBool::new(false);

    struct MyActor;

    #[cfg_attr(feature = "async-trait", async_trait::async_trait)]
    impl Actor for MyActor {
        type Msg = String;
        type State = ActorRef<WatchdogMsg>;
        type Arguments = ActorRef<WatchdogMsg>;

        async fn pre_start(
            &self,
            myself: ActorRef<Self::Msg>,
            watchdog: Self::Arguments,
        ) -> Result<Self::State, ActorProcessingErr> {
            cast!(
                watchdog.clone(),
                Register(
                    myself.get_cell(),
                    Duration::from_millis(500),
                    TimeoutStrategy::Kill
                )
            )?;

            myself.send_after(Duration::from_millis(400), || "hello".to_string());

            Ok(watchdog)
        }

        async fn post_stop(
            &self,
            _: ActorRef<Self::Msg>,
            _: &mut Self::State,
        ) -> Result<(), ActorProcessingErr> {
            POST_STOP.store(true, SeqCst);
            Ok(())
        }

        async fn handle(
            &self,
            myself: ActorRef<Self::Msg>,
            msg: Self::Msg,
            state: &mut Self::State,
        ) -> Result<(), ActorProcessingErr> {
            info!("handle() msg={}", msg);
            HANDLE.store(true, SeqCst);
            cast!(
                state,
                Register(
                    myself.get_cell(),
                    Duration::from_millis(500),
                    TimeoutStrategy::Kill
                )
            )
            .map_err(|e| ActorProcessingErr::from(e))
        }
    }

    info!("starting");
    let (watchdog, watchdog_handle) = Actor::spawn(None, Watchdog, ()).await.unwrap();
    info!("watchdog started");

    info!("starting my_actor");
    let (my_actor, my_actor_handle) = Actor::spawn(None, MyActor, watchdog.clone()).await.unwrap();
    info!("my_actor started");

    tokio::time::sleep(Duration::from_millis(100)).await;

    assert_eq!(false, HANDLE.load(SeqCst));
    assert_eq!(ActorStatus::Running, my_actor.get_status());

    tokio::time::sleep(Duration::from_millis(3000)).await;

    assert_eq!(true, HANDLE.load(SeqCst));
    assert_eq!(false, POST_STOP.load(SeqCst));
    assert_eq!(ActorStatus::Stopped, my_actor.get_status());
    let stats = watchdog
        .call(|port| Stats(port), None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(1, stats.kills);

    my_actor_handle.await.unwrap();

    watchdog.stop(None);

    watchdog_handle.await.unwrap();
}
