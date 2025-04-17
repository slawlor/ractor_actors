// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

extern crate ractor_actors;

use chrono::{DateTime, Utc};
use ractor::{Actor, ActorProcessingErr, ActorRef};
use ractor_actors::net::tcp::*;
use ractor_actors::watchdog;
use ractor_actors::watchdog::TimeoutStrategy;
use std::error::Error;
use std::str::FromStr;
use std::time::SystemTime;

struct MyServer;

struct MyServerArgs {
    /// Controls if the watchdog is used
    watchdog: bool,

    /// The port to listen on
    port: NetworkPort,
}

#[cfg_attr(feature = "async-trait", async_trait::async_trait)]
impl Actor for MyServer {
    type Msg = ();
    type State = ();
    type Arguments = MyServerArgs;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        MyServerArgs { watchdog, port }: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let acceptor = MyServerSocketAcceptor { watchdog };

        let _ = myself
            .spawn_linked(
                Some(format!("listener-{}", port)),
                Listener::new(),
                ListenerStartupArgs {
                    port,
                    encryption: IncomingEncryptionMode::Raw,
                    acceptor,
                },
            )
            .await?;

        Ok(())
    }
}

struct MyServerSocketAcceptor {
    watchdog: bool,
}

#[cfg_attr(feature = "async-trait", async_trait::async_trait)]
impl SessionAcceptor for MyServerSocketAcceptor {
    async fn new_session(&self, stream: NetworkStream) -> Result<(), ActorProcessingErr> {
        tracing::info!("New connection: {}", stream.peer_addr());
        Actor::spawn(
            Some(format!("MySession-{}", stream.peer_addr().port())),
            MySession {
                info: stream.info(),
            },
            MySessionArgs {
                watchdog: self.watchdog,
                stream,
            },
        )
        .await?;

        Ok(())
    }
}

struct MySession {
    info: NetworkStreamInfo,
}

enum MySessionMsg {
    FrameReady(Frame),
}

struct MySessionState {
    watchdog: bool,
    session: ActorRef<TcpSessionMessage>,
}

struct MySessionArgs {
    watchdog: bool,
    stream: NetworkStream,
}

struct MyFrameReceiver {
    session: ActorRef<MySessionMsg>,
}

#[cfg_attr(feature = "async-trait", async_trait::async_trait)]
impl FrameReceiver for MyFrameReceiver {
    async fn frame_ready(&self, f: Frame) -> Result<(), ActorProcessingErr> {
        self.session.cast(MySessionMsg::FrameReady(f))?;

        Ok(())
    }
}

#[cfg_attr(feature = "async-trait", async_trait::async_trait)]
impl Actor for MySession {
    type Msg = MySessionMsg;
    type State = MySessionState;
    type Arguments = MySessionArgs;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        Self::Arguments { watchdog, stream }: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        tracing::info!("New session: {}", stream.peer_addr());

        let receiver = MyFrameReceiver {
            session: myself.clone(),
        };

        let (session, _) = TcpSession::spawn_linked(
            None,
            TcpSession::default(),
            TcpSessionStartupArguments {
                receiver,
                tcp_session: stream,
            },
            myself.get_cell(),
        )
        .await?;

        if watchdog {
            watchdog::register(
                myself.get_cell(),
                ractor::concurrency::Duration::from_secs(3),
                TimeoutStrategy::Stop,
            )
            .await?;
        }

        Ok(Self::State { watchdog, session })
    }

    async fn post_stop(
        &self,
        _: ActorRef<Self::Msg>,
        _: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        tracing::info!("Stopping connection for {}", self.info.peer_addr);

        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            Self::Msg::FrameReady(frame) => {
                if state.watchdog {
                    watchdog::ping(myself.get_id()).await?;
                }

                let s: String = String::from_utf8(frame).unwrap();
                tracing::info!("Got message: {:?}", s);

                let ts: DateTime<Utc> = SystemTime::now().into();
                let reply = format!("{}: {}", ts.to_rfc3339(), s);

                state
                    .session
                    .cast(TcpSessionMessage::Send(reply.into_bytes()))?;

                Ok(())
            }
        }
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
    let port = NetworkPort::from_str(&s)?;

    let watchdog = std::env::var("WATCHDOG").unwrap_or("0".to_string()) == "1";

    tracing::info!("Listening on port {}. Watchdog: {}", port, watchdog);
    tracing::info!("watchdog: {}", watchdog);

    let (system_ref, _) = Actor::spawn(None, MyServer {}, MyServerArgs { watchdog, port }).await?;

    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for Ctrl-C");

    tracing::info!("fin.");

    system_ref.stop_and_wait(None, None).await?;

    Ok(())
}
