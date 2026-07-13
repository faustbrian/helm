use super::EngineError;
use std::time::Duration;

const MINIMUM_DURATION: Duration = Duration::from_millis(1);

/// A validated, non-shell health check for one managed container.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ContainerHealthCheck {
    command: Vec<String>,
    interval_nanoseconds: i64,
    timeout_nanoseconds: i64,
    start_period_nanoseconds: i64,
    retries: i64,
}

impl ContainerHealthCheck {
    pub(crate) fn new(
        command: Vec<String>,
        interval: Duration,
        timeout: Duration,
        start_period: Duration,
        retries: u32,
    ) -> Result<Self, EngineError> {
        if command.first().is_none_or(String::is_empty)
            || command.iter().any(|argument| argument.contains('\0'))
        {
            return Err(EngineError::InvalidRequest {
                detail:
                    "container health check must contain a non-empty executable and no NUL bytes"
                        .to_owned(),
            });
        }
        if retries == 0 {
            return Err(EngineError::InvalidRequest {
                detail: "container health check retries must be greater than zero".to_owned(),
            });
        }

        Ok(Self {
            command,
            interval_nanoseconds: duration_nanoseconds("interval", interval)?,
            timeout_nanoseconds: duration_nanoseconds("timeout", timeout)?,
            start_period_nanoseconds: duration_nanoseconds("start period", start_period)?,
            retries: i64::from(retries),
        })
    }

    pub(crate) fn engine_test(&self) -> Vec<String> {
        std::iter::once("CMD".to_owned())
            .chain(self.command.iter().cloned())
            .collect()
    }

    pub(crate) const fn interval_nanoseconds(&self) -> i64 {
        self.interval_nanoseconds
    }

    pub(crate) const fn timeout_nanoseconds(&self) -> i64 {
        self.timeout_nanoseconds
    }

    pub(crate) const fn start_period_nanoseconds(&self) -> i64 {
        self.start_period_nanoseconds
    }

    pub(crate) const fn retries(&self) -> i64 {
        self.retries
    }
}

fn duration_nanoseconds(kind: &str, duration: Duration) -> Result<i64, EngineError> {
    if duration < MINIMUM_DURATION {
        return Err(EngineError::InvalidRequest {
            detail: format!("container health check {kind} must be at least 1 millisecond"),
        });
    }

    i64::try_from(duration.as_nanos()).map_err(|_| EngineError::InvalidRequest {
        detail: format!("container health check {kind} exceeds the Engine duration limit"),
    })
}
