use super::{CommandRequest, EngineError};
use std::fmt::{Debug, Formatter};
use std::time::Duration;

/// Bounded metadata for one attached command with streamed input and output.
pub(crate) struct StreamingCommandOptions {
    request: CommandRequest,
    action: String,
    timeout: Duration,
}

impl StreamingCommandOptions {
    pub(crate) fn new(
        request: CommandRequest,
        action: impl Into<String>,
        timeout: Duration,
    ) -> Result<Self, EngineError> {
        let action = action.into();
        if action.is_empty() || timeout.is_zero() {
            return Err(EngineError::InvalidRequest {
                detail: "streaming command requires an action and non-zero timeout".to_owned(),
            });
        }

        Ok(Self {
            request,
            action,
            timeout,
        })
    }

    pub(super) const fn request(&self) -> &CommandRequest {
        &self.request
    }

    pub(super) fn action(&self) -> &str {
        &self.action
    }

    pub(super) const fn timeout(&self) -> Duration {
        self.timeout
    }
}

impl Debug for StreamingCommandOptions {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StreamingCommandOptions")
            .field("request", &self.request)
            .field("action", &self.action)
            .field("timeout", &self.timeout)
            .finish()
    }
}
