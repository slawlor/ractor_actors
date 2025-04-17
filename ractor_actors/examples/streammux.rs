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
//! cargo run --example streammux
//! ```

extern crate ractor_actors;

use std::marker::PhantomData;

use ractor::ActorProcessingErr;
use ractor_actors::streams::mux::*;
use tokio_stream::{Stream, StreamExt};

struct MyTarget<S> {
    idx: u16,
    _s: PhantomData<fn() -> S>,
}

impl<S> Target<S> for MyTarget<S>
where
    S: Stream<Item = String> + ractor::State,
{
    fn get_id(&self) -> String {
        format!("{}", self.idx)
    }

    fn message_received(&self, msg: String) -> Result<(), ActorProcessingErr> {
        tracing::debug!("Target: {} - Msg: {msg}", self.idx);
        Ok(())
    }
}

struct MyCallback;

impl StreamMuxNotification for MyCallback {
    fn target_failed(&self, _target: String, _err: ActorProcessingErr) {
        tracing::error!("Target {_target} failed with error {_err:?}")
    }
    fn end_of_stream(&self) {
        tracing::info!("End of stream reached")
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
        .fmt_fields(GlogFields::default().compact());

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

    let stream = tokio_stream::iter(0..10u32).map(|i| format!("Message {i}"));

    let targets = vec![
        MyTarget {
            idx: 0,
            _s: PhantomData,
        },
        MyTarget {
            idx: 1,
            _s: PhantomData,
        },
        MyTarget {
            idx: 2,
            _s: PhantomData,
        },
    ]
    .into_iter()
    .map(|target| Box::new(target) as Box<dyn Target<_>>)
    .collect::<Vec<_>>();

    let config = StreamMuxConfiguration {
        stream,
        stop_processing_target_on_failure: true,
        targets,
        callback: MyCallback,
    };

    let muxer = mux_stream(config, None)
        .await
        .expect("Failed to start mutiplexer");

    // The muxer should die after the stream ends, signifying the test is over
    muxer
        .drain_and_wait(None)
        .await
        .expect("Failed to drain muxer");
    tracing::info!("fin.");
}
