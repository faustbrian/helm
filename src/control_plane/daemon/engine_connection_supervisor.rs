use super::{
    EngineConnectionOutcome, EngineConnectionSupervisorError, EngineConnector, RetryBackoff,
    RetryDelay,
};
use std::path::PathBuf;
use std::time::Instant;

/// Quietly maintains one exact installation-selected Engine connection.
pub(crate) struct EngineConnectionSupervisor<Connector>
where
    Connector: EngineConnector,
{
    connector: Connector,
    endpoint: PathBuf,
    retry: RetryBackoff,
    retry_state: Option<(Instant, RetryDelay)>,
    engine: Option<Connector::Engine>,
}

impl<Connector> EngineConnectionSupervisor<Connector>
where
    Connector: EngineConnector,
{
    pub(crate) fn new(
        connector: Connector,
        endpoint: PathBuf,
        retry: RetryBackoff,
    ) -> Result<Self, EngineConnectionSupervisorError> {
        if !endpoint.is_absolute() {
            return Err(EngineConnectionSupervisorError::new(
                "Engine endpoint path must be absolute",
            ));
        }

        Ok(Self {
            connector,
            endpoint,
            retry,
            retry_state: None,
            engine: None,
        })
    }

    /// Connects when due, otherwise returns the current bounded backoff state.
    pub(crate) async fn poll(&mut self, now: Instant) -> EngineConnectionOutcome {
        if self.engine.is_some() {
            return EngineConnectionOutcome::Connected;
        }
        if let Some((deadline, retry)) = self.retry_state
            && now < deadline
        {
            return EngineConnectionOutcome::BackingOff { retry };
        }

        match self.connector.connect(&self.endpoint).await {
            Ok(engine) => {
                self.engine = Some(engine);
                self.retry_state = None;
                self.retry.reset();
                EngineConnectionOutcome::Connected
            }
            Err(error) => {
                let retry = self.retry.next_delay();
                self.retry_state = Some((now + retry.duration(), retry));
                EngineConnectionOutcome::Unavailable {
                    retry,
                    detail: error.to_string(),
                }
            }
        }
    }

    /// Provides the connected Engine for capability-oriented reconciliation.
    pub(crate) fn engine_mut(&mut self) -> Option<&mut Connector::Engine> {
        self.engine.as_mut()
    }

    /// Borrows the connected adapter for independently owned read/exec clones.
    pub(crate) const fn engine(&self) -> Option<&Connector::Engine> {
        self.engine.as_ref()
    }

    /// Reports whether this supervisor currently holds a usable adapter.
    pub(crate) const fn is_connected(&self) -> bool {
        self.engine.is_some()
    }

    /// Drops a failed adapter and schedules one bounded reconnect attempt.
    pub(crate) fn invalidate(&mut self, now: Instant) -> RetryDelay {
        self.engine = None;
        let retry = self.retry.next_delay();
        self.retry_state = Some((now + retry.duration(), retry));

        retry
    }
}
