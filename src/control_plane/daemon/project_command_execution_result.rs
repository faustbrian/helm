use crate::control_plane::engine::{AttachedCommandOutput, EngineError};

/// Terminal result returned by one asynchronous project Engine exec task.
pub(crate) struct ProjectCommandExecutionResult {
    operation_id: String,
    outcome: Result<AttachedCommandOutput, EngineError>,
}

impl ProjectCommandExecutionResult {
    pub(crate) const fn new(
        operation_id: String,
        outcome: Result<AttachedCommandOutput, EngineError>,
    ) -> Self {
        Self {
            operation_id,
            outcome,
        }
    }

    pub(crate) fn operation_id(&self) -> &str {
        &self.operation_id
    }

    pub(crate) const fn outcome(&self) -> &Result<AttachedCommandOutput, EngineError> {
        &self.outcome
    }

    pub(crate) fn into_parts(self) -> (String, Result<AttachedCommandOutput, EngineError>) {
        (self.operation_id, self.outcome)
    }
}
