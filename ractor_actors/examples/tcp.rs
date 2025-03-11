// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

extern crate ractor_actors;

use chrono::{DateTime, Utc};
use ractor::{Actor, ActorProcessingErr, ActorRef, DerivedActorRef, SupervisionEvent};
use ractor_actors::net::tcp::*;
use std::error::Error;
use std::str::FromStr;
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
        let _ = myself
            .spawn_linked(
                Some("listener".into()),
                Listener::new(
                    port,
                    IncomingEncryptionMode::Raw,
                    move |supervisor, session, info: NetworkStreamInfo| async move {
                        tracing::info!("New connection: {}", info.peer_addr);
                        tracing::info!("supervised by: {:?}", supervisor);
                        let (session, _) =
                            Actor::spawn_linked(Some(format!("{}-session", info.peer_addr)), MySession { session, info }, (), supervisor)
                                .await
                                .map_err(|e| ActorProcessingErr::from(e))?;
                        let s: DerivedActorRef<FrameAvailable> = session.get_derived();
                        Ok(s)
                    },
                ),
                (),
            )
            .await?;

        Ok(())
    }
}

struct MySession {
    session: DerivedActorRef<SendFrame>,
    info: NetworkStreamInfo,
}

enum MySessionMsg {
    FrameAvailable(Frame),
}

impl From<FrameAvailable> for MySessionMsg {
    fn from(FrameAvailable(frame): FrameAvailable) -> Self {
        Self::FrameAvailable(frame)
    }
}

impl TryFrom<MySessionMsg> for FrameAvailable {
    type Error = ();

    fn try_from(_: MySessionMsg) -> Result<Self, Self::Error> {
        tracing::info!("woot");
        Err(())
    }
}

impl Actor for MySession {
    type Msg = MySessionMsg;
    type State = ();
    type Arguments = ();

    async fn pre_start(
        &self,
        _: ActorRef<Self::Msg>,
        _: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        tracing::info!("New session: {}", self.info.peer_addr);
        Ok(())
    }

    async fn handle(
        &self,
        _: ActorRef<Self::Msg>,
        message: Self::Msg,
        _: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            Self::Msg::FrameAvailable(frame) => {
                let s: String = String::from_utf8(frame).unwrap();
                tracing::info!("Got message: {:?}", s);

                let ts: DateTime<Utc> = SystemTime::now().into();
                let reply = format!("{}: {}", ts.to_rfc3339(), s);

                let _ = self.session.cast(SendFrame(reply.into_bytes()))?;

                Ok(())
            }
        }
    }

    async fn handle_supervisor_evt(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: SupervisionEvent,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        tracing::info!("handle_supervisor_evt: {:?}", message);

        Ok(())
    }
}

fn init_logging() {
    use std::io::stderr;
    use std::io::IsTerminal;
    use tracing::level_filters::LevelFilter;
    use tracing_subscriber::filter::EnvFilter;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::Registry;

    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::DEBUG.into())
        .from_env_lossy();

    let fmt = tracing_subscriber::fmt::Layer::default()
        .with_ansi(stderr().is_terminal())
        .with_writer(stderr)
        .event_format(tracing_subscriber::fmt::format().compact())
        // .event_format(Glog::default().with_timer(tracing_glog::LocalTime::default()))
        // .fmt_fields(GlogFields::default().compact())
        ;

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
