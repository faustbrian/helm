use super::QueuedPostgresPrune;
use std::path::PathBuf;
use std::time::Duration;

/// Runtime-only inputs for one crash-replayable PostgreSQL prune operation.
pub(crate) struct PostgresPruneExecutionOptions {
    pub(crate) operation: QueuedPostgresPrune,
    pub(crate) state_database_path: PathBuf,
    pub(crate) installation_id: String,
    pub(crate) schema_version: u32,
    pub(crate) timeout: Duration,
}
