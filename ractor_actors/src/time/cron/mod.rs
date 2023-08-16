// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

//! Cron job management, including cron supervison
//!
//! NOTE: We do not currently support re-spawning failed cron jobs because the
//! [Job] trait is not cloneable. Given we're utilizing a [Box] implementation, we cannot
//! straightforward implement cloning of the job logic without some added boiler plate.
//!
//! In order to restart a failed cron job in the [CronManager], we need both a [Schedule]
//! and a [Job] which at this time, we don't have yet.

use std::collections::HashMap;

use cron::Schedule;
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort, State, SupervisionEvent};

mod worker;
use worker::{Cron, CronMessage};

/// Represents a job managed by a cron schedule. Executes on a
/// given period and may take an unknown amount of time.
///
/// If the job takes longer than the period, queueing may occur and the
/// job may violate scheduling
#[async_trait::async_trait]
pub trait Job: State {
    /// Retrieve the name of the cron job for logging
    fn id<'a>(&self) -> &'a str;

    /// Execute the work, taking a mutable reference to the object
    async fn work(&mut self) -> Result<(), ActorProcessingErr>;
}

/// The settings for a singular cron job
pub struct CronSettings {
    /// This cron job's schedule
    pub schedule: Schedule,
    /// The job logic and identifier
    pub job: Box<dyn Job>,
}

/// The [CronManager] is responsible for managing a pool of [Cron] jobs
/// and dynamically modifying the pool as needed
pub struct CronManager;

/// The state of the [CronManager] actor
pub struct CronManagerState {
    jobs: HashMap<String, (Schedule, ActorRef<CronMessage>)>,
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
}

#[async_trait::async_trait]
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
            SupervisionEvent::ActorPanicked(who, what) => {
                let job = state
                    .jobs
                    .iter()
                    .find(|(_, v)| v.1.get_id() == who.get_id())
                    .map(|(id, _)| id.clone());
                if let Some(name) = job {
                    tracing::error!("Cron job {name} panicked with error {what}.");
                    state.jobs.remove(&name);
                }
            }
            SupervisionEvent::ActorTerminated(who, _, _) => {
                // just cleanup if it's still hanging around
                let job = state
                    .jobs
                    .iter()
                    .find(|(_, v)| v.1.get_id() == who.get_id())
                    .map(|(id, _)| id.clone());
                if let Some(name) = job {
                    state.jobs.remove(&name);
                }
            }
            _ => {
                // ignore all other supervision events (spawn, etc)
            }
        }
        Ok(())
    }
}
