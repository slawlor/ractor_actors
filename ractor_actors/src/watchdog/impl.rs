use super::{TimeoutStrategy, WatchdogStats, WATCHDOG_TIMEOUT};
use ractor::concurrency::{Duration, JoinHandle};
use ractor::{
    ActorCell, ActorId, ActorProcessingErr, ActorRef, MessagingErr, RpcReplyPort, SupervisionEvent,
};
use std::collections::HashMap;
use tracing::{debug, info};

pub struct Watchdog;

pub enum WatchdogMsg {
    Register(ActorCell, Duration, TimeoutStrategy),
    Unregister(ActorCell),
    Ping(ActorId, RpcReplyPort<()>),
    Timeout(ActorId),
    Stats(RpcReplyPort<WatchdogStats>),
}

pub struct WatchdogState {
    subjects: HashMap<ActorId, Registration>,
    kills: usize,
    stops: usize,
}

struct Registration {
    actor: ActorCell,
    timeout: Duration,
    timeout_strategy: TimeoutStrategy,
    timer: JoinHandle<Result<(), MessagingErr<WatchdogMsg>>>,
}

#[ractor::actor(message = WatchdogMsg, state = WatchdogState)]
impl Watchdog {
    async fn pre_start(
        &self,
        _: ActorRef<Self::Msg>,
        _: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(WatchdogState {
            subjects: HashMap::new(),
            kills: 0,
            stops: 0,
        })
    }

    #[ractor::message(WatchdogMsg::Register(actor, timeout, timeout_strategy))]
    fn register(
        &self,
        myself: ActorRef<WatchdogMsg>,
        actor: ActorCell,
        timeout: Duration,
        timeout_strategy: TimeoutStrategy,
        state: &mut WatchdogState,
    ) {
        let id = actor.get_id();
        let timer = myself.send_after(timeout, move || WatchdogMsg::Timeout(id));

        state.subjects.insert(
            id,
            Registration {
                actor,
                timeout,
                timeout_strategy,
                timer,
            },
        );
    }

    #[ractor::message(WatchdogMsg::Unregister(actor))]
    fn unregister(&self, actor: ActorCell, state: &mut WatchdogState) {
        state.unregister(&actor);
    }

    #[ractor::message(WatchdogMsg::Ping(actor, reply))]
    fn ping(
        &self,
        myself: ActorRef<WatchdogMsg>,
        actor: ActorId,
        reply: RpcReplyPort<()>,
        state: &mut WatchdogState,
    ) {
        if let Some(Registration { timeout, timer, .. }) = state.subjects.get(&actor) {
            info!(actor = actor.to_string(), "got ping, rescheduling watchdog");
            timer.abort();
            myself.send_after(*timeout, move || WatchdogMsg::Timeout(actor));

            // Ignore reply failures so subject shutdown cannot affect the watchdog.
            let _ = reply.send(());
        } else {
            state.subjects.remove(&actor);
        }
    }

    #[ractor::message(WatchdogMsg::Timeout(actor))]
    fn timeout(&self, actor: ActorId, state: &mut WatchdogState) {
        if let Some(Registration {
            actor,
            timeout_strategy,
            ..
        }) = state.subjects.remove(&actor)
        {
            match timeout_strategy {
                TimeoutStrategy::Kill => {
                    info!(
                        actor_id = actor.get_id().to_string(),
                        actor_name = actor.get_name(),
                        "watchdog timeout, killing",
                    );
                    actor.kill();
                    state.kills += 1;
                }
                TimeoutStrategy::Stop => {
                    info!(
                        actor_id = actor.get_id().to_string(),
                        actor_name = actor.get_name(),
                        "watchdog timeout, stopping",
                    );
                    actor.stop(Some(WATCHDOG_TIMEOUT.to_string()));
                    state.stops += 1;
                }
            }
        };
    }

    #[ractor::message(WatchdogMsg::Stats(reply))]
    fn stats(
        &self,
        reply: RpcReplyPort<WatchdogStats>,
        state: &WatchdogState,
    ) -> Result<(), ActorProcessingErr> {
        reply
            .send(WatchdogStats { kills: state.kills })
            .map_err(ActorProcessingErr::from)
    }

    async fn handle_supervisor_evt(
        &self,
        _: ActorRef<Self::Msg>,
        message: SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SupervisionEvent::ActorTerminated(cell, ..) => {
                debug!(actor = cell.get_id().to_string(), "actor terminated");
                state.unregister(&cell);
                Ok(())
            }
            SupervisionEvent::ActorFailed(cell, ..) => {
                debug!(actor = cell.get_id().to_string(), "actor failed");
                state.unregister(&cell);
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

impl WatchdogState {
    fn unregister(&mut self, cell: &ActorCell) -> Option<ActorCell> {
        debug!(actor = cell.get_id().to_string(), "unregistering");
        self.subjects
            .remove(&cell.get_id())
            .map(|Registration { actor, timer, .. }| {
                timer.abort();
                actor
            })
    }
}
