use ractor::{ActorProcessingErr, State};

/// Represents a job managed by a cron schedule. Executes on a
/// given period and may take an unknown amount of time.
///
/// If the job takes longer than the period, queueing may occur and the
/// job may violate scheduling
#[async_trait::async_trait]
pub trait Job: State {
    /// Retrieve the name of the cron job for logging
    fn id<'a>(&self) -> &'a str;

    /// Execute the work, taking a mutable reference to the object
    async fn work(&mut self) -> Result<(), ActorProcessingErr>;
}

/// Represents a dynamic subscription to [CronManager] events which
/// denote cron job startup/failure/stoppage
///
/// [CronManager]: crate::time::cron_0_16::CronManager
pub trait CronEventSubscriber: State {
    /// A job started
    fn started(&self, job: String);

    /// A job was stopped, with the optional stop reason
    fn stopped(&self, job: String, reason: Option<String>);

    /// A job failed, including the failure reason
    fn failed(&self, job: String, reason: String);
}
