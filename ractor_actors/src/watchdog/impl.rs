use super::{TimeoutStrategy, WatchdogStats, WATCHDOG_TIMEOUT};
use ractor::concurrency::{Duration, JoinHandle};
use ractor::{
    Actor, ActorCell, ActorId, ActorProcessingErr, ActorRef, MessagingErr, RpcReplyPort,
    SupervisionEvent,
};
use std::collections::HashMap;
use tracing::{debug, info};

pub struct Watchdog;

pub enum WatchdogMsg {
    Register(ActorCell, Duration, TimeoutStrategy),
    Unregister(ActorCell),
    Ping(ActorId),
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

#[cfg_attr(feature = "async-trait", async_trait::async_trait)]
impl Actor for Watchdog {
    type Msg = WatchdogMsg;
    type State = WatchdogState;
    type Arguments = ();

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

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            WatchdogMsg::Register(actor, timeout, timeout_strategy) => {
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
                Ok(())
            }
            WatchdogMsg::Unregister(actor) => {
                state.unregister(&actor);
                Ok(())
            }
            WatchdogMsg::Ping(actor) => match state.subjects.get(&actor) {
                Some(Registration { timeout, timer, .. }) => {
                    info!(actor = actor.to_string(), "got ping, rescheduling watchdog");
                    timer.abort();
                    myself.send_after(*timeout, move || WatchdogMsg::Timeout(actor));
                    Ok(())
                }
                _ => {
                    state.subjects.remove(&actor);
                    Ok(())
                }
            },
            WatchdogMsg::Timeout(actor) => {
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
                Ok(())
            }
            WatchdogMsg::Stats(reply) => reply
                .send(WatchdogStats { kills: state.kills })
                .map_err(ActorProcessingErr::from),
        }
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
