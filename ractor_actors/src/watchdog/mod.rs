use ractor::concurrency::Duration;
use ractor::rpc::CallResult;
use ractor::{Actor, ActorId, ActorRef, MessagingErr, RpcReplyPort};
use ractor::{ActorCell, ActorProcessingErr};
use tokio::sync::OnceCell;
use watchdog::WatchdogMsg;

pub enum TimeoutStrategy {
    Kill,
    Stop,
}

pub const WATCHDOG_TIMEOUT: &'static str = "watchdog_timeout";

pub async fn register(
    actor: ActorCell,
    duration: Duration,
    timeout_strategy: TimeoutStrategy,
) -> Result<(), MessagingErr<()>> {
    cast(WatchdogMsg::Register(actor, duration, timeout_strategy)).await
}

pub async fn unregister(actor: ActorCell) -> Result<(), MessagingErr<()>> {
    cast(WatchdogMsg::Unregister(actor)).await
}

pub async fn ping(actor: ActorId) -> Result<(), MessagingErr<()>> {
    cast(WatchdogMsg::Ping(actor)).await
}

pub struct WatchdogStats {
    pub kills: usize,
}

pub async fn stats() -> Result<WatchdogStats, MessagingErr<()>> {
    call(WatchdogMsg::Stats).await
}

static WATCHDOG: OnceCell<Result<ActorRef<WatchdogMsg>, ActorProcessingErr>> =
    OnceCell::const_new();

async fn spawn() -> Result<ActorRef<WatchdogMsg>, ActorProcessingErr> {
    let (watchdog, _) = Actor::spawn(None, watchdog::Watchdog {}, ()).await?;

    Ok(watchdog)
}

async fn cast(msg: WatchdogMsg) -> Result<(), MessagingErr<()>> {
    match WATCHDOG.get_or_init(spawn).await {
        Ok(watchdog) => watchdog.cast(msg).map_err(|_| MessagingErr::SendErr(())),
        Err(_) => Err(MessagingErr::SendErr(())),
    }
}

async fn call<TReply, TMsgBuilder>(msg_builder: TMsgBuilder) -> Result<TReply, MessagingErr<()>>
where
    TMsgBuilder: FnOnce(RpcReplyPort<TReply>) -> WatchdogMsg,
{
    match WATCHDOG.get_or_init(spawn).await {
        Ok(watchdog) => match watchdog
            .call(msg_builder, None)
            .await
            .map_err(|_| MessagingErr::SendErr(()))?
        {
            CallResult::Success(result) => Ok(result),
            _ => Err(MessagingErr::SendErr(())),
        },
        Err(_) => Err(MessagingErr::SendErr(())),
    }
}

mod watchdog;

#[cfg(test)]
pub mod tests;
