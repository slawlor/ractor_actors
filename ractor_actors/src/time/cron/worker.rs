// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

//! Cron-job actor for scheduling periodic jobs

use chrono::Utc;
use cron::Schedule;
use ractor::{concurrency::JoinHandle, Actor, ActorProcessingErr, ActorRef, MessagingErr};

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

pub enum CronMessage {
    Execute,
    UpdateSchedule(Schedule),
}

#[ractor::async_trait]
impl Actor for Cron {
    type Msg = CronMessage;
    type State = CronState;
    type Arguments = CronSettings;

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

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: CronMessage,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            CronMessage::Execute => {
                tracing::debug!("Executing cron job {}", state.job.id());

                if let Err(e) = state.job.work().await {
                    tracing::error!("Cron job {} failed with error {e}", state.job.id());
                }

                // schedule next ping
                state.schedule_next(&myself)?;
            }
            CronMessage::UpdateSchedule(new_schedule) => {
                if let Some(h) = state.next_schedule_handle.take() {
                    h.abort();
                    drop(h);
                }
                state.schedule = new_schedule;
                // schedule next ping on the "new" schedule
                state.schedule_next(&myself)?;
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

    use ractor::concurrency::{sleep, Duration};

    use super::*;

    struct TestJob {
        counter: Arc<AtomicU16>,
    }

    #[ractor::async_trait]
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

        sleep(Duration::from_secs(4)).await;

        assert!(counter.load(Ordering::Relaxed) >= 3);
        assert!(counter.load(Ordering::Relaxed) < 5);

        actor.stop(None);
        handle.await.unwrap();
    }
}
