use super::QueuedPostgresPrune;

/// Terminal outcome after Engine deletion and durable-state retirement.
pub(crate) struct PostgresPruneExecutionResult {
    operation: QueuedPostgresPrune,
    outcome: Result<(), String>,
}

impl PostgresPruneExecutionResult {
    pub(crate) const fn new(operation: QueuedPostgresPrune, outcome: Result<(), String>) -> Self {
        Self { operation, outcome }
    }

    #[cfg(test)]
    pub(crate) const fn outcome(&self) -> &Result<(), String> {
        &self.outcome
    }

    pub(crate) fn into_parts(self) -> (QueuedPostgresPrune, Result<(), String>) {
        (self.operation, self.outcome)
    }
}
