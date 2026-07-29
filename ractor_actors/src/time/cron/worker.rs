// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

//! Cron-job actor for scheduling periodic jobs

use chrono::Utc;
use cron::Schedule;
use ractor::{concurrency::JoinHandle, ActorProcessingErr, ActorRef, MessagingErr};

#[cfg(test)]
use ractor::Actor;

use super::{CronSettings, Job};

pub struct Cron;

pub struct CronState {
    schedule: Schedule,
    job: Box<dyn Job>,
    next_schedule_handle: Option<JoinHandle<Result<(), MessagingErr<CronMessage>>>>,
}

impl CronState {
    fn schedule_next(&mut self, myself: &ActorRef<CronMessage>) -> Result<(), ActorProcessingErr> {
        let handle = if let Some(next) = self.schedule.upcoming(Utc).next() {
            let period = next - Utc::now();
            Some(myself.send_after(period.to_std()?, || CronMessage::Execute))
        } else {
            None
        };
        self.next_schedule_handle = handle;
        Ok(())
    }
}

#[allow(clippy::large_enum_variant)]
pub enum CronMessage {
    Execute,
    UpdateSchedule(Schedule),
}

#[ractor::actor(
    message = CronMessage,
    state = CronState,
    arguments = CronSettings,
)]
impl Cron {
    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        CronSettings { schedule, job }: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        // schedule first ping
        let handle = if let Some(next) = schedule.upcoming(Utc).next() {
            let period = next - Utc::now();
            Some(myself.send_after(period.to_std()?, || CronMessage::Execute))
        } else {
            None
        };

        Ok(CronState {
            schedule,
            job,
            next_schedule_handle: handle,
        })
    }

    async fn post_stop(
        &self,
        _: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // we're exiting, cleanup any pending send action
        if let Some(handle) = state.next_schedule_handle.take() {
            handle.abort();
            drop(handle);
        }
        Ok(())
    }

    #[ractor::message(CronMessage::Execute)]
    async fn execute(
        &self,
        myself: ActorRef<CronMessage>,
        state: &mut CronState,
    ) -> Result<(), ActorProcessingErr> {
        tracing::debug!("Executing cron job {}", state.job.id());

        if let Err(error) = state.job.work().await {
            tracing::error!("Cron job {} failed with error {error}", state.job.id());
        }

        // schedule next ping
        state.schedule_next(&myself)
    }

    #[ractor::message(CronMessage::UpdateSchedule(new_schedule))]
    fn update_schedule(
        &self,
        myself: ActorRef<CronMessage>,
        new_schedule: Schedule,
        state: &mut CronState,
    ) -> Result<(), ActorProcessingErr> {
        if let Some(handle) = state.next_schedule_handle.take() {
            handle.abort();
            drop(handle);
        }
        state.schedule = new_schedule;
        // schedule next ping on the new schedule
        state.schedule_next(&myself)
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

    use crate::common_test::periodic_check;

    use super::*;

    struct TestJob {
        counter: Arc<AtomicU16>,
    }

    #[async_trait::async_trait]
    impl Job for TestJob {
        fn id<'a>(&self) -> &'a str {
            "test_job"
        }

        async fn work(&mut self) -> Result<(), ActorProcessingErr> {
            self.counter.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }

    #[ractor::concurrency::test]
    #[tracing_test::traced_test]
    async fn test_cron_job() {
        //                    sec  min   hour   day of month   month   day of week   year
        let schedule = " */1    *     *         *            *          *          *";
        let schedule = Schedule::from_str(schedule).expect("Failed to parse schedule");

        let counter = Arc::new(AtomicU16::new(0));
        let job = TestJob {
            counter: counter.clone(),
        };

        let (actor, handle) = Actor::spawn(
            None,
            Cron,
            CronSettings {
                schedule,
                job: Box::new(job),
            },
        )
        .await
        .expect("Failed to start cron job");

        periodic_check(
            || counter.load(Ordering::Relaxed) >= 3 && counter.load(Ordering::Relaxed) < 5,
            Duration::from_secs(10),
        )
        .await;

        actor.stop(None);
        handle.await.unwrap();
    }
}
