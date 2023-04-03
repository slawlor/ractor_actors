// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

use std::env::temp_dir;
use std::io::Write;
use std::sync::{Arc, Mutex};

use ractor::concurrency::{test as rtest, Duration};

use super::*;

#[rtest]
async fn filewatcher_starts_default_config() {
    // Setup
    let fw = FileWatcher;
    let config = FileWatcherConfig {
        ..Default::default()
    };
    let (fwactor, fwhandle) = Actor::spawn(None, fw, config)
        .await
        .expect("Filewatcher failed to spawn");

    // Cleanup
    fwactor.stop(None);
    fwhandle.await.unwrap();
}

#[rtest]
async fn filewatch_watches_file() {
    // Setup
    let mut dir = temp_dir();
    let file_name = "filewatch_watches_file.txt".to_string();
    dir.push(file_name);

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(dir.clone())
        .expect("Failed to create temp file");
    write!(file, "Some data").expect("Failed to write to file");
    drop(file); // close out the file

    let fw = FileWatcher;
    let config = FileWatcherConfig {
        files: vec![dir.clone()],
        ..Default::default()
    };
    let (fwactor, fwhandle) = Actor::spawn(None, fw, config)
        .await
        .expect("Filewatcher failed to spawn");

    // Act
    let events = Arc::new(Mutex::new(Vec::<String>::new()));

    // subscribe to the actor's event stream
    struct Subsciption {
        ev: Arc<Mutex<Vec<String>>>,
    }

    impl FileWatcherSubscriber for Subsciption {
        fn event_received(&self, ev: notify::Event) {
            self.ev.lock().unwrap().push(format!("{:?}", ev.kind));
        }
    }

    let subscription = Box::new(Subsciption { ev: events.clone() });
    let sub_result = ractor::call_t!(
        fwactor,
        FileWatcherMessage::Subscribe,
        200,
        fwactor.get_id(),
        subscription
    )
    .expect("RPC failed");
    assert!(sub_result == SubscriptionResult::Ok);

    // Trigger an event
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(dir)
        .expect("Failed to create temp file");
    write!(file, "Some data").expect("Failed to write to file");
    drop(file); // close out the file

    ractor::concurrency::sleep(Duration::from_millis(200)).await;

    assert!(events.lock().unwrap().len() >= 1);

    // Cleanup
    fwactor.stop(None);
    fwhandle.await.unwrap();
}
