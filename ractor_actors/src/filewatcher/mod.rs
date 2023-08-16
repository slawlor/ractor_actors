// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

//! A file watcher actor which monitors for specific paths notifying of changes
extern crate notify;

use std::{collections::HashMap, path::PathBuf};

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use ractor::{Actor, ActorId, ActorProcessingErr, ActorRef, RpcReplyPort};

#[cfg(test)]
mod tests;

/// A filewatcher actor
pub struct FileWatcher;

#[derive(Clone, Copy, Eq, PartialEq, Hash)]
pub enum SubscriptionResult {
    Ok,
    Duplicate,
    NotFound,
}

/// File watcher messages
pub enum FileWatcherMessage {
    /// A file event
    Event(Event),
    /// A file-watcher error occurred, kill the actor
    FwError(notify::Error),
    /// Subscribe to file events
    Subscribe(
        ActorId,
        Box<dyn FileWatcherSubscriber>,
        RpcReplyPort<SubscriptionResult>,
    ),
    /// Unsubscribe to file events
    Unsubscribe(ActorId, RpcReplyPort<SubscriptionResult>),
}

/// File watcher subscription to events
pub trait FileWatcherSubscriber: Send + 'static {
    /// Called when an [Event] is received from the file watcher
    fn event_received(&self, ev: Event);
}

/// Configuration for a file watcher
#[derive(Default)]
pub struct FileWatcherConfig {
    /// Files to watch
    pub files: Vec<PathBuf>,
    /// Directories to watch
    pub directories: Vec<PathBuf>,
}

pub struct FileWatcherState {
    watcher: Option<RecommendedWatcher>,
    subscriptions: HashMap<ActorId, Box<dyn FileWatcherSubscriber>>,
}

#[async_trait::async_trait]
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
                log::error!("Filewatcher error: {:?}", e);
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
