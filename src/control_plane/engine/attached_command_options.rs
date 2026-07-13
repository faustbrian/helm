use super::{CommandRequest, EngineError};
use std::fmt::{Debug, Formatter};
use std::time::Duration;

/// Complete bounded input for one attached non-shell Engine command.
pub(crate) struct AttachedCommandOptions {
    request: CommandRequest,
    input: Vec<u8>,
    action: String,
    timeout: Duration,
}

impl AttachedCommandOptions {
    pub(crate) fn new(
        request: CommandRequest,
        input: Vec<u8>,
        action: impl Into<String>,
        timeout: Duration,
    ) -> Result<Self, EngineError> {
        let action = action.into();
        if action.is_empty() || timeout.is_zero() {
            return Err(EngineError::InvalidRequest {
                detail: "attached command requires an action and non-zero timeout".to_owned(),
            });
        }

        Ok(Self {
            request,
            input,
            action,
            timeout,
        })
    }

    pub(super) const fn request(&self) -> &CommandRequest {
        &self.request
    }

    pub(super) fn input(&self) -> &[u8] {
        &self.input
    }

    pub(super) fn action(&self) -> &str {
        &self.action
    }

    pub(super) const fn timeout(&self) -> Duration {
        self.timeout
    }
}

impl Debug for AttachedCommandOptions {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AttachedCommandOptions")
            .field("request", &self.request)
            .field("input", &"[REDACTED]")
            .field("action", &self.action)
            .field("timeout", &self.timeout)
            .finish()
    }
}
