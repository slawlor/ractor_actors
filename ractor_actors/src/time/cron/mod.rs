// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

//! Cron job management, including cron supervison provided by the [CronManager] actor
//!
//! NOTE: We do not currently support re-spawning failed cron jobs because the
//! [Job] trait is not cloneable. Given we're utilizing a [Box] implementation, we cannot
//! straightforward implement cloning of the job logic without some added boiler plate.
//!
//! In order to restart a failed cron job in the [CronManager], we need both a [Schedule]
//! and a [Job] which at this time, we don't have yet.
//!
//! # Example usage:
//!
//! ```rust
//! use std::str::FromStr;
//! use std::time::Duration;
//!
//! use cron::Schedule;
//! use ractor::{async_trait, Actor, ActorProcessingErr};
//! use ractor_actors::time::cron::*;
//!
//! struct SomeJob;
//!
//! #[async_trait]
//! impl Job for SomeJob {
//!     fn id<'a>(&self) -> &'a str {
//!         "some_job"
//!     }
//!     async fn work(&mut self) -> Result<(), ActorProcessingErr> {
//!         println!("Some job doing something");
//!         Ok(())
//!     }
//! }
//!
//! async fn example() {
//!     // Execute the job every 30s
//!     let schedule = " */30    *     *         *            *          *          *";
//!     let schedule = Schedule::from_str(schedule).expect("Failed to parse schedule");
//!     let (manager, _mhandle) = Actor::spawn(None, CronManager, ())
//!         .await
//!         .expect("Failed to spawn cron manager");
//!     
//!     // Act & Verify
//!     manager
//!         .call(
//!             |prt| {
//!                 CronManagerMessage::Start(
//!                     CronSettings {
//!                         schedule,
//!                         job: Box::new(SomeJob),
//!                     },
//!                     prt,
//!                 )
//!             },
//!             Some(Duration::from_millis(100)),
//!         )
//!         .await
//!         .expect("Failed to send start message")
//!         .expect("Cron send timed out")
//!         .expect("Failed to start cron job with error");
//!     // cleanup the manager or keep it somewhere
//! }
//! ```

use std::collections::HashMap;

use cron::Schedule;
use ractor::{Actor, ActorId, ActorProcessingErr, ActorRef, RpcReplyPort, State, SupervisionEvent};

mod worker;
use worker::{Cron, CronMessage};

/// Represents a job managed by a cron schedule. Executes on a
/// given period and may take an unknown amount of time.
///
/// If the job takes longer than the period, queueing may occur and the
/// job may violate scheduling
#[ractor::async_trait]
pub trait Job: State {
    /// Retrieve the name of the cron job for logging
    fn id<'a>(&self) -> &'a str;

    /// Execute the work, taking a mutable reference to the object
    async fn work(&mut self) -> Result<(), ActorProcessingErr>;
}

/// Represents a dynamic subscription to [CronManager] events which
/// denote cron job startup/failure/stoppage
pub trait CronEventSubscriber: State {
    /// A job started
    fn started(&self, job: String);

    /// A job was stopped, with the optional stop reason
    fn stopped(&self, job: String, reason: Option<String>);

    /// A job failed, including the failure reason
    fn failed(&self, job: String, reason: String);
}

/// The settings for a singular cron job
pub struct CronSettings {
    /// This cron job's schedule
    pub schedule: Schedule,
    /// The job logic and identifier
    pub job: Box<dyn Job>,
}

/// The [CronManager] is responsible for managing a pool of cron [Job]s
/// and dynamically modifying the pool as needed
pub struct CronManager;

/// The state of the [CronManager] actor, containing all of the
/// jobs to schedule
pub struct CronManagerState {
    jobs: HashMap<String, (Schedule, ActorRef<CronMessage>)>,
    subs: HashMap<ActorId, Box<dyn CronEventSubscriber>>,
}

/// Messages that the [CronManager] actor supports
pub enum CronManagerMessage {
    /// List the currently scheduled cron jobs
    ListJobs(RpcReplyPort<HashMap<String, Schedule>>),
    /// Retrieve the schedule for a given cron job if the job exists
    GetSchedule(String, RpcReplyPort<Option<Schedule>>),
    /// Set the new schedule for a specific cron job
    SetSchedule(String, Schedule),
    /// Start a new cron job with the specified settings
    Start(CronSettings, RpcReplyPort<Result<(), ActorProcessingErr>>),
    /// Stop a specific cron job
    Stop(String),
    /// Subscribe to cron job events
    Subscribe(ActorId, Box<dyn CronEventSubscriber>),
    /// Unsubscriber from cron job events
    Unsubscribe(ActorId),
}

#[ractor::async_trait]
impl Actor for CronManager {
    type Msg = CronManagerMessage;
    type State = CronManagerState;
    type Arguments = ();

    async fn pre_start(
        &self,
        _: ActorRef<Self::Msg>,
        _: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(CronManagerState {
            jobs: HashMap::new(),
            subs: HashMap::new(),
        })
    }

    async fn post_stop(
        &self,
        _: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        for (_, actor) in state.jobs.iter() {
            actor.1.stop(None);
        }
        state.jobs.clear();
        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: CronManagerMessage,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            CronManagerMessage::Start(settings, reply) => {
                let id = settings.job.id().to_string();
                let sched = settings.schedule.clone();

                if let std::collections::hash_map::Entry::Vacant(e) = state.jobs.entry(id) {
                    match Actor::spawn_linked(None, Cron, settings, myself.get_cell()).await {
                        Err(spawn_err) => {
                            let _ = reply.send(Err(spawn_err.into()));
                        }
                        Ok((actor, _)) => {
                            e.insert((sched, actor));
                            let _ = reply.send(Ok(()));
                        }
                    }
                } else {
                    let _ = reply.send(Err(From::from(
                        "A job with the name {} already is scheduled",
                    )));
                }
            }
            CronManagerMessage::Stop(who) => {
                if let Some(actor) = state.jobs.remove(&who) {
                    actor.1.stop(None);
                    for (_, sub) in state.subs.iter() {
                        sub.stopped(who.clone(), None);
                    }
                }
            }
            CronManagerMessage::SetSchedule(who, schedule) => {
                if let Some(actor) = state.jobs.get_mut(&who) {
                    actor.0 = schedule.clone();
                    actor.1.cast(CronMessage::UpdateSchedule(schedule))?;
                }
            }
            CronManagerMessage::ListJobs(reply) => {
                let msg = state
                    .jobs
                    .iter()
                    .map(|(name, job_state)| (name.clone(), job_state.0.clone()))
                    .collect::<HashMap<_, _>>();
                let _ = reply.send(msg);
            }
            CronManagerMessage::GetSchedule(who, reply) => {
                if let Some(actor) = state.jobs.get(&who) {
                    let _ = reply.send(Some(actor.0.clone()));
                } else {
                    let _ = reply.send(None);
                }
            }
            CronManagerMessage::Subscribe(who, processor) => {
                state.subs.insert(who, processor);
            }
            CronManagerMessage::Unsubscribe(who) => {
                state.subs.remove(&who);
            }
        }
        Ok(())
    }

    async fn handle_supervisor_evt(
        &self,
        _: ActorRef<Self::Msg>,
        evt: SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match evt {
            SupervisionEvent::ActorFailed(who, what) => {
                let job = state
                    .jobs
                    .iter()
                    .find(|(_, v)| v.1.get_id() == who.get_id())
                    .map(|(id, _)| id.clone());
                if let Some(name) = job {
                    tracing::error!("Cron job {name} panicked with error {what}.");
                    for (_, sub) in state.subs.iter() {
                        sub.failed(name.clone(), what.to_string());
                    }
                    state.jobs.remove(&name);
                }
            }
            SupervisionEvent::ActorTerminated(who, _, what) => {
                // just cleanup if it's still hanging around
                let job = state
                    .jobs
                    .iter()
                    .find(|(_, v)| v.1.get_id() == who.get_id())
                    .map(|(id, _)| id.clone());
                if let Some(name) = job {
                    for (_, sub) in state.subs.iter() {
                        sub.stopped(name.clone(), what.clone());
                    }
                    state.jobs.remove(&name);
                }
            }
            SupervisionEvent::ActorStarted(who) => {
                let job = state
                    .jobs
                    .iter()
                    .find(|(_, v)| v.1.get_id() == who.get_id())
                    .map(|(id, _)| id.clone());
                if let Some(name) = job {
                    for (_, sub) in state.subs.iter() {
                        sub.started(name.clone());
                    }
                }
            }
            _ => {
                // ignore all other supervision events (spawn, etc)
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        str::FromStr,
        sync::{
            atomic::{AtomicU16, Ordering},
            Arc,
        },
    };

    use ractor::concurrency::Duration;

    use crate::common_test::{periodic_async_check, periodic_check};

    use super::*;

    struct BadJob;
    #[ractor::async_trait]
    impl Job for BadJob {
        fn id<'a>(&self) -> &'a str {
            "bad_job"
        }
        #[allow(clippy::diverging_sub_expression)]
        async fn work(&mut self) -> Result<(), ActorProcessingErr> {
            panic!("Boom!");
        }
    }

    struct CounterJob {
        counter: Arc<AtomicU16>,
    }
    #[ractor::async_trait]
    impl Job for CounterJob {
        fn id<'a>(&self) -> &'a str {
            "counter_job"
        }
        async fn work(&mut self) -> Result<(), ActorProcessingErr> {
            self.counter.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }

    #[ractor::concurrency::test]
    #[tracing_test::traced_test]
    async fn test_cron_lifecycle() {
        // Setup
        let schedule = " */1    *     *         *            *          *          *";
        let schedule = Schedule::from_str(schedule).expect("Failed to parse schedule");
        let counter = Arc::new(AtomicU16::new(0));
        let (manager, mhandle) = Actor::spawn(None, CronManager, ())
            .await
            .expect("Failed to spawn cron manager");
        let counter_job = CounterJob {
            counter: counter.clone(),
        };

        // Act & Verify
        manager
            .call(
                |prt| {
                    CronManagerMessage::Start(
                        CronSettings {
                            schedule,
                            job: Box::new(counter_job),
                        },
                        prt,
                    )
                },
                Some(Duration::from_millis(100)),
            )
            .await
            .expect("Failed to send start message")
            .expect("Cron send timed out")
            .expect("Failed to start cron job with error");

        let result = ractor::call_t!(manager, CronManagerMessage::ListJobs, 100)
            .expect("Failed to query jobs list");
        assert!(result.contains_key("counter_job"));

        // check job is running and cron is executing
        periodic_check(
            || counter.load(Ordering::Relaxed) >= 3 && counter.load(Ordering::Relaxed) < 5,
            Duration::from_secs(5),
        )
        .await;

        manager
            .cast(CronManagerMessage::Stop("counter_job".to_string()))
            .expect("Failed to contact cron manager");
        let result = ractor::call_t!(manager, CronManagerMessage::ListJobs, 100)
            .expect("Failed to query jobs list");
        assert!(!result.contains_key("counter_job"));

        // Cleanup
        manager.stop(None);
        mhandle.await.unwrap();
    }

    #[ractor::concurrency::test]
    #[tracing_test::traced_test]
    async fn test_failing_cronjob() {
        // Setup
        let schedule = " */1    *     *         *            *          *          *";
        let schedule = Schedule::from_str(schedule).expect("Failed to parse schedule");
        let (manager, mhandle) = Actor::spawn(None, CronManager, ())
            .await
            .expect("Failed to spawn cron manager");

        // Act & Verify
        manager
            .call(
                |prt| {
                    CronManagerMessage::Start(
                        CronSettings {
                            schedule,
                            job: Box::new(BadJob),
                        },
                        prt,
                    )
                },
                Some(Duration::from_millis(100)),
            )
            .await
            .expect("Failed to send start message")
            .expect("Cron send timed out")
            .expect("Failed to start cron job with error");

        let result = ractor::call_t!(manager, CronManagerMessage::ListJobs, 100)
            .expect("Failed to query jobs list");
        assert!(result.contains_key("bad_job"));

        // check job has failed
        periodic_async_check(
            || async {
                let result = ractor::call_t!(manager, CronManagerMessage::ListJobs, 100)
                    .expect("Failed to query jobs list");
                !result.contains_key("bad_job")
            },
            Duration::from_secs(4),
        )
        .await;

        // Cleanup
        manager.stop(None);
        mhandle.await.unwrap();
    }

    #[ractor::concurrency::test]
    #[tracing_test::traced_test]
    async fn test_cron_event_subscription() {
        // Setup
        let schedule = " */1    *     *         *            *          *          *";
        let schedule = Schedule::from_str(schedule).expect("Failed to parse schedule");
        let start_counter = Arc::new(AtomicU16::new(0));
        let stop_counter = Arc::new(AtomicU16::new(0));
        let fail_counter = Arc::new(AtomicU16::new(0));
        let counter = Arc::new(AtomicU16::new(0));

        struct Subscriber {
            starts: Arc<AtomicU16>,
            stops: Arc<AtomicU16>,
            fails: Arc<AtomicU16>,
        }

        impl CronEventSubscriber for Subscriber {
            fn started(&self, _: String) {
                self.starts.fetch_add(1, Ordering::Relaxed);
            }
            fn stopped(&self, _: String, _: Option<String>) {
                self.stops.fetch_add(1, Ordering::Relaxed);
            }
            fn failed(&self, _: String, _: String) {
                self.fails.fetch_add(1, Ordering::Relaxed);
            }
        }

        let (manager, mhandle) = Actor::spawn(None, CronManager, ())
            .await
            .expect("Failed to spawn cron manager");

        // Act & Verify
        manager
            .cast(CronManagerMessage::Subscribe(
                ActorId::Local(123),
                Box::new(Subscriber {
                    fails: fail_counter.clone(),
                    starts: start_counter.clone(),
                    stops: stop_counter.clone(),
                }),
            ))
            .expect("Failed to send message to manager");

        manager
            .call(
                |prt| {
                    CronManagerMessage::Start(
                        CronSettings {
                            schedule: schedule.clone(),
                            job: Box::new(CounterJob { counter }),
                        },
                        prt,
                    )
                },
                None,
            )
            .await
            .expect("Failed to send start message")
            .expect("Cron send timed out")
            .expect("Failed to start cron job with error");

        manager
            .call(
                |prt| {
                    CronManagerMessage::Start(
                        CronSettings {
                            schedule,
                            job: Box::new(BadJob),
                        },
                        prt,
                    )
                },
                Some(Duration::from_millis(100)),
            )
            .await
            .expect("Failed to send start message")
            .expect("Cron send timed out")
            .expect("Failed to start cron job with error");

        periodic_check(
            || {
                start_counter.load(Ordering::Relaxed) == 2
                    && fail_counter.load(Ordering::Relaxed) == 1
            },
            Duration::from_secs(5),
        )
        .await;

        manager
            .cast(CronManagerMessage::Stop("counter_job".to_string()))
            .expect("Failed to send stop command");
        periodic_check(
            || stop_counter.load(Ordering::Relaxed) == 1,
            Duration::from_secs(5),
        )
        .await;

        // cleanup cron manager
        manager.stop(None);
        mhandle.await.unwrap();
    }
}
