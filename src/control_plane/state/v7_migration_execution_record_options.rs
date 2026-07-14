use super::{V7MigrationAdapterCheckpoint, V7MigrationExecutionPhase};
use std::path::PathBuf;

/// Complete fields for one project-wide reversible v7 execution barrier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct V7MigrationExecutionRecordOptions {
    pub(crate) project_id: String,
    pub(crate) canonical_project_path: PathBuf,
    pub(crate) evidence_revision: String,
    pub(crate) adapter_plan_revision: String,
    pub(crate) phase: V7MigrationExecutionPhase,
    pub(crate) checkpoints: Vec<V7MigrationAdapterCheckpoint>,
    pub(crate) updated_at_unix_seconds: i64,
}
