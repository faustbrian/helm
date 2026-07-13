use super::MigrationPhase;

/// Complete durable identity and proof fields for one migration checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MigrationRecordOptions {
    pub(crate) migration_id: String,
    pub(crate) project_id: String,
    pub(crate) source_revision: String,
    pub(crate) target_revision: String,
    pub(crate) source_compatibility_fingerprint: String,
    pub(crate) target_compatibility_fingerprint: String,
    pub(crate) phase: MigrationPhase,
    pub(crate) backup_artifact_sha256: Option<String>,
    pub(crate) backup_artifact_size_bytes: Option<u64>,
    pub(crate) target_resource_id: Option<String>,
    pub(crate) rollback_reference: Option<String>,
    pub(crate) updated_at_unix_seconds: i64,
}
