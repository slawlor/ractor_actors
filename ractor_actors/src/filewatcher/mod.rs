// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

//! A [FileWatcher] actor which monitors for specific paths notifying of changes.
//! This watcher requires the use of the `notify` crate to monitor the host filesystem
//! for changes.

extern crate notify;

use std::{collections::HashMap, path::PathBuf};

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use ractor::{Actor, ActorId, ActorProcessingErr, ActorRef, RpcReplyPort};

#[cfg(test)]
mod tests;

/// An actor which watches for file changes on the host OS. It is
/// responsible for managing subscriptions to file changes and notifying
/// subscribers of received changes.
pub struct FileWatcher;

#[derive(Clone, Copy, Eq, PartialEq, Hash)]
pub enum SubscriptionResult {
    /// Subscription operation succeeded
    Ok,
    /// There is already a subscription for the specified actor
    Duplicate,
    /// Upon removal, this subscription was not found.
    NotFound,
}

/// Messages supported by the [FileWatcher] actor
pub enum FileWatcherMessage {
    /// A file event was received
    Event(Event),
    /// A file-watcher error occurred. Terminates the actor
    FwError(notify::Error),
    /// Subscribe the specified actor to file events using
    /// the provided subscription logic
    Subscribe(
        ActorId,
        Box<dyn FileWatcherSubscriber>,
        RpcReplyPort<SubscriptionResult>,
    ),
    /// Unsubscribe the given actor from file events
    Unsubscribe(ActorId, RpcReplyPort<SubscriptionResult>),
}

/// Represents a subscription to file events
pub trait FileWatcherSubscriber: Send + 'static {
    /// Called when an [Event] is received from the file watcher
    fn event_received(&self, ev: Event);
}

/// Configuration for a file watcher
#[derive(Default)]
pub struct FileWatcherConfig {
    /// List of files to watch
    pub files: Vec<PathBuf>,
    /// List of directories to watch
    pub directories: Vec<PathBuf>,
}

/// The state of the [FileWatcher] actor
pub struct FileWatcherState {
    watcher: Option<RecommendedWatcher>,
    subscriptions: HashMap<ActorId, Box<dyn FileWatcherSubscriber>>,
}

#[ractor::async_trait]
impl Actor for FileWatcher {
    type Msg = FileWatcherMessage;
    type State = FileWatcherState;
    type Arguments = FileWatcherConfig;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        arguments: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        // Automatically select the best implementation for your platform.
        let mut watcher = notify::recommended_watcher(move |res| match res {
            Ok(event) => {
                let _ = myself.cast(Self::Msg::Event(event));
            }
            Err(e) => {
                let _ = myself.cast(Self::Msg::FwError(e));
            }
        })?;

        for file in arguments.files {
            watcher.watch(&file, RecursiveMode::NonRecursive)?
        }

        for dir in arguments.directories {
            watcher.watch(&dir, RecursiveMode::Recursive)?
        }

        Ok(FileWatcherState {
            watcher: Some(watcher),
            subscriptions: HashMap::new(),
        })
    }

    async fn post_stop(
        &self,
        _: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        if let Some(w) = state.watcher.take() {
            drop(w);
        }
        Ok(())
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            FileWatcherMessage::Event(watcher_event) => {
                for sub in state.subscriptions.values() {
                    sub.event_received(watcher_event.clone());
                }
            }
            FileWatcherMessage::FwError(e) => {
                tracing::error!("Filewatcher error: {:?}", e);
                return Err(e.into());
            }
            FileWatcherMessage::Subscribe(who, f, reply) => {
                if state.subscriptions.insert(who, f).is_none() {
                    let _ = reply.send(SubscriptionResult::Ok);
                } else {
                    let _ = reply.send(SubscriptionResult::Duplicate);
                }
            }
            FileWatcherMessage::Unsubscribe(who, reply) => {
                if state.subscriptions.remove(&who).is_some() {
                    let _ = reply.send(SubscriptionResult::Ok);
                } else {
                    let _ = reply.send(SubscriptionResult::NotFound);
                }
            }
        }
        Ok(())
    }
}
