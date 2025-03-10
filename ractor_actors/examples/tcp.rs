// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

extern crate ractor_actors;

use chrono::{DateTime, Utc};
use ractor::{Actor, ActorProcessingErr, ActorRef};
use ractor_actors::net::tcp::*;
use std::error::Error;
use std::ops::Deref;
use std::str::FromStr;
use std::sync::Arc;
use std::time::SystemTime;

struct MyServer {}

impl Actor for MyServer {
    type Msg = ();
    type State = ();
    type Arguments = NetworkPort;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        port: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let cell = Arc::new(myself.get_cell());

        let _ = myself
            .spawn_linked(
                Some("listener".into()),
                Listener::new(
                    port,
                    move |stream: NetworkStream| {
                        let cell = cell.clone();
                        async move {
                            tracing::info!("New connection: {}", stream.peer_addr());
                            let _ = Actor::spawn_linked(
                                None,
                                MySession {},
                                stream,
                                cell.deref().clone(),
                            )
                            .await
                            .map_err(|e| ActorProcessingErr::from(e))?;
                            Ok(())
                        }
                    },
                    IncomingEncryptionMode::Raw,
                ),
                (),
            )
            .await?;

        Ok(())
    }
}

struct MySession {}

impl Actor for MySession {
    type Msg = SessionMessage;
    type State = ActorRef<SessionMessage>;
    type Arguments = NetworkStream;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        stream: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let peer_addr = stream.peer_addr();
        let local_addr = stream.local_addr();
        let session = Session::spawn_linked(
            myself.clone(),
            stream,
            peer_addr,
            local_addr,
            myself.get_cell(),
        )
        .await
        .map_err(|e| ActorProcessingErr::from(e))?;

        Ok(session)
    }

    async fn handle(
        &self,
        _: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SessionMessage::Send(frame) => {
                tracing::info!("Sending frame {:?}??", frame);
                Ok(())
            }
            SessionMessage::FrameAvailable(frame) => {
                let s: String = String::from_utf8(frame).unwrap();
                tracing::info!("Got message: {:?}", s);

                let ts: DateTime<Utc> = SystemTime::now().into();
                let reply = format!("{}: {}", ts.to_rfc3339(), s);

                let _ = state.cast(SessionMessage::Send(reply.into_bytes()))?;

                Ok(())
            }
        }
    }
}

fn init_logging() {
    use std::io::stderr;
    use std::io::IsTerminal;
    use tracing::level_filters::LevelFilter;
    use tracing_glog::Glog;
    use tracing_glog::GlogFields;
    use tracing_subscriber::filter::EnvFilter;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::Registry;

    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::DEBUG.into())
        .from_env_lossy();

    let fmt = tracing_subscriber::fmt::Layer::default()
        .with_ansi(stderr().is_terminal())
        .with_writer(std::io::stderr)
        .event_format(Glog::default().with_timer(tracing_glog::LocalTime::default()))
        .fmt_fields(GlogFields::default().compact());

    let subscriber = Registry::default().with(filter).with(fmt);
    tracing::subscriber::set_global_default(subscriber).expect("to set global subscriber");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_logging();

    let s = std::env::var("PORT").unwrap_or("9999".to_string());
    let port: NetworkPort = NetworkPort::from_str(&s)?;

    let (system_ref, _) = Actor::spawn(None, MyServer {}, port).await?;

    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for Ctrl-C");

    tracing::info!("fin.");

    system_ref.stop_and_wait(None, None).await?;

    Ok(())
}
