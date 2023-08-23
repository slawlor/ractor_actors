// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

//! A basic counting agent. Demonstrates remote procedure calls to interact
//! with the agent externally and safely acquire the "count"
//!
//! Execute with
//!
//! ```text
//! cargo run --example cron
//! ```

extern crate ractor_actors;
use std::str::FromStr;

use cron::Schedule;
use ractor::{
    async_trait,
    concurrency::{sleep, Duration, Instant},
    Actor, ActorProcessingErr,
};
use ractor_actors::time::cron::{CronManager, CronManagerMessage, CronSettings, Job};

struct MyJob {
    last: Option<Instant>,
}

#[async_trait]
impl Job for MyJob {
    fn id<'a>(&self) -> &'a str {
        "my_job"
    }

    async fn work(&mut self) -> Result<(), ActorProcessingErr> {
        let now = Instant::now();
        let delta = self.last.map(|ts| (now - ts).as_millis());

        sleep(Duration::from_millis(500)).await;

        tracing::info!("Working hard for {:?} ms", delta);

        self.last = Some(now);

        Ok(())
    }
}

fn init_logging() {
    let dir = tracing_subscriber::filter::Directive::from(tracing::Level::DEBUG);

    use std::io::stderr;
    use std::io::IsTerminal;
    use tracing_glog::Glog;
    use tracing_glog::GlogFields;
    use tracing_subscriber::filter::EnvFilter;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::Registry;

    let fmt = tracing_subscriber::fmt::Layer::default()
        .with_ansi(stderr().is_terminal())
        .with_writer(std::io::stderr)
        .event_format(Glog::default().with_timer(tracing_glog::LocalTime::default()))
        .fmt_fields(GlogFields);

    let filter = vec![dir]
        .into_iter()
        .fold(EnvFilter::from_default_env(), |filter, directive| {
            filter.add_directive(directive)
        });

    let subscriber = Registry::default().with(filter).with(fmt);
    tracing::subscriber::set_global_default(subscriber).expect("to set global subscriber");
}

#[tokio::main]
async fn main() {
    init_logging();

    // every 5s
    let schedule = "*/5 * * * * * *";
    let schedule = Schedule::from_str(schedule).expect("Failed to parse schedule");

    let (manager, handle) = Actor::spawn(None, CronManager, ())
        .await
        .expect("Failed to start cron manager");

    manager
        .call(
            |prt| {
                CronManagerMessage::Start(
                    CronSettings {
                        job: Box::new(MyJob { last: None }),
                        schedule,
                    },
                    prt,
                )
            },
            None,
        )
        .await
        .expect("Failed to contact cron manager")
        .expect("Failed to send rpc reply")
        .expect("Failed to start cron job");

    // cleanup
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to wait for ctrl-c");
    manager.stop(None);
    handle.await.unwrap();
}
