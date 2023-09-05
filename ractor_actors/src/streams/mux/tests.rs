// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

use std::{
    marker::PhantomData,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};

use ractor::concurrency::Duration;
use ractor::ActorStatus;
use tokio_stream::Iter;

use super::*;
use crate::common_test::periodic_check;

// ================= Happy Path ================= //

struct CounterTarget<S> {
    idx: u16,
    counter: Arc<AtomicU32>,
    _s: PhantomData<S>,
}

impl<S> Target<S> for CounterTarget<S>
where
    S: Stream + ractor::State,
    S::Item: Clone + ractor::Message,
{
    fn get_id(&self) -> String {
        format!("{}", self.idx)
    }

    fn message_received(&self, _: <S as Stream>::Item) -> Result<(), ActorProcessingErr> {
        self.counter.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

struct DummyCallback;

impl StreamMuxNotification for DummyCallback {
    fn target_failed(&self, _target: String, _err: ActorProcessingErr) {}
    fn end_of_stream(&self) {}
}

#[ractor::concurrency::test]
#[tracing_test::traced_test]
async fn test_mux_publishing() {
    let counter = Arc::new(AtomicU32::new(0));

    let targets: Vec<Box<dyn Target<Iter<std::ops::Range<u32>>>>> = vec![
        Box::new(CounterTarget {
            idx: 0,
            counter: counter.clone(),
            _s: PhantomData,
        }),
        Box::new(CounterTarget {
            idx: 1,
            counter: counter.clone(),
            _s: PhantomData,
        }),
        Box::new(CounterTarget {
            idx: 2,
            counter: counter.clone(),
            _s: PhantomData,
        }),
    ];

    let config = StreamMuxConfiguration {
        stream: tokio_stream::iter(0..10u32),
        stop_processing_target_on_failure: true,
        targets,
        callback: DummyCallback,
    };

    let muxer = mux_stream(config, None)
        .await
        .expect("Failed to start mutiplexer");

    // The muxer should die after the stream ends, signifying the test is over
    periodic_check(
        || muxer.get_status() == ActorStatus::Stopped,
        Duration::from_secs(5),
    )
    .await;

    assert_eq!(30, counter.load(Ordering::Relaxed));
}

// ================= Failing Targets ================= //

struct GoodTarget;

impl<S> Target<S> for GoodTarget
where
    S: Stream + ractor::State,
    S::Item: Clone + ractor::Message,
{
    fn get_id(&self) -> String {
        "good".to_string()
    }

    fn message_received(&self, _: <S as Stream>::Item) -> Result<(), ActorProcessingErr> {
        Ok(())
    }
}

struct BadTarget;

impl<S> Target<S> for BadTarget
where
    S: Stream + ractor::State,
    S::Item: Clone + ractor::Message,
{
    fn get_id(&self) -> String {
        "bad".to_string()
    }

    fn message_received(&self, _: <S as Stream>::Item) -> Result<(), ActorProcessingErr> {
        Err(From::from("boom"))
    }
}

struct CountingCallback {
    fail_counter: Arc<AtomicU32>,
    eos_counter: Arc<AtomicU32>,
}

impl StreamMuxNotification for CountingCallback {
    fn target_failed(&self, _target: String, _err: ActorProcessingErr) {
        self.fail_counter.fetch_add(1, Ordering::Relaxed);
    }
    fn end_of_stream(&self) {
        self.eos_counter.fetch_add(1, Ordering::Relaxed);
    }
}

#[ractor::concurrency::test]
#[tracing_test::traced_test]
async fn test_mux_failed_targets() {
    let fail_counter = Arc::new(AtomicU32::new(0));
    let eos_counter = Arc::new(AtomicU32::new(0));

    let targets: Vec<Box<dyn Target<Iter<std::ops::Range<u32>>>>> =
        vec![Box::new(GoodTarget), Box::new(BadTarget)];

    let config = StreamMuxConfiguration {
        stream: tokio_stream::iter(0..10u32),
        stop_processing_target_on_failure: true,
        targets,
        callback: CountingCallback {
            fail_counter: fail_counter.clone(),
            eos_counter: eos_counter.clone(),
        },
    };

    let muxer = mux_stream(config, None)
        .await
        .expect("Failed to start mutiplexer");

    // wait for muxer to stop
    periodic_check(
        || muxer.get_status() == ActorStatus::Stopped,
        Duration::from_secs(5),
    )
    .await;

    assert_eq!(1, fail_counter.load(Ordering::Relaxed));
    assert_eq!(1, eos_counter.load(Ordering::Relaxed));
}

#[ractor::concurrency::test]
#[tracing_test::traced_test]
async fn test_mux_failed_targets_no_removal() {
    let fail_counter = Arc::new(AtomicU32::new(0));
    let eos_counter = Arc::new(AtomicU32::new(0));

    let targets: Vec<Box<dyn Target<Iter<std::ops::Range<u32>>>>> =
        vec![Box::new(GoodTarget), Box::new(BadTarget)];

    let config = StreamMuxConfiguration {
        stream: tokio_stream::iter(0..10u32),
        stop_processing_target_on_failure: false,
        targets,
        callback: CountingCallback {
            fail_counter: fail_counter.clone(),
            eos_counter: eos_counter.clone(),
        },
    };

    let muxer = mux_stream(config, None)
        .await
        .expect("Failed to start mutiplexer");

    // wait for muxer to stop
    periodic_check(
        || muxer.get_status() == ActorStatus::Stopped,
        Duration::from_secs(5),
    )
    .await;

    assert_eq!(10, fail_counter.load(Ordering::Relaxed));
    assert_eq!(1, eos_counter.load(Ordering::Relaxed));
}
