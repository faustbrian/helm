/// Exact reason accepted v7 evidence cannot produce a safe adapter plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct V7MigrationAdapterSelectionError {
    message: String,
}

impl V7MigrationAdapterSelectionError {
    pub(super) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for V7MigrationAdapterSelectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for V7MigrationAdapterSelectionError {}
