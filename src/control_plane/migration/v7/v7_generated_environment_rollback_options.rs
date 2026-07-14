use super::V7GeneratedEnvironmentArtifact;
use std::path::Path;

/// Exact source evidence and private destination for one environment capture.
pub(crate) struct V7GeneratedEnvironmentRollbackOptions<'operation> {
    pub(crate) project_id: &'operation str,
    pub(crate) evidence_revision: &'operation str,
    pub(crate) expected: &'operation V7GeneratedEnvironmentArtifact,
    pub(crate) backup_root: &'operation Path,
    pub(crate) maximum_environment_bytes: usize,
    pub(crate) created_at_unix_seconds: i64,
}
