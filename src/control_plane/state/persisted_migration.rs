use super::{MigrationPhase, MigrationRecord, MigrationRecordOptions, StateStoreError};

/// Untyped SQLite values awaiting migration-record validation.
pub(super) struct PersistedMigration {
    pub(super) migration_id: String,
    pub(super) project_id: String,
    pub(super) source_revision: String,
    pub(super) target_revision: String,
    pub(super) source_compatibility_fingerprint: String,
    pub(super) target_compatibility_fingerprint: String,
    pub(super) phase: String,
    pub(super) backup_reference: Option<String>,
    pub(super) backup_artifact_sha256: Option<String>,
    pub(super) backup_artifact_size_bytes: Option<i64>,
    pub(super) target_resource_id: Option<String>,
    pub(super) rollback_reference: Option<String>,
    pub(super) updated_at_unix_seconds: i64,
}

impl PersistedMigration {
    pub(super) fn into_record(self) -> Result<MigrationRecord, StateStoreError> {
        let phase =
            MigrationPhase::parse(&self.phase).ok_or_else(|| StateStoreError::CorruptState {
                detail: format!(
                    "migration '{}' has unknown phase '{}'",
                    self.migration_id, self.phase
                ),
            })?;
        let backup_artifact_size_bytes = self
            .backup_artifact_size_bytes
            .map(u64::try_from)
            .transpose()
            .map_err(|_| StateStoreError::CorruptState {
                detail: format!(
                    "migration '{}' has a negative backup size",
                    self.migration_id
                ),
            })?;

        MigrationRecord::new(MigrationRecordOptions {
            migration_id: self.migration_id,
            project_id: self.project_id,
            source_revision: self.source_revision,
            target_revision: self.target_revision,
            source_compatibility_fingerprint: self.source_compatibility_fingerprint,
            target_compatibility_fingerprint: self.target_compatibility_fingerprint,
            phase,
            backup_reference: self.backup_reference,
            backup_artifact_sha256: self.backup_artifact_sha256,
            backup_artifact_size_bytes,
            target_resource_id: self.target_resource_id,
            rollback_reference: self.rollback_reference,
            updated_at_unix_seconds: self.updated_at_unix_seconds,
        })
        .map_err(|error| StateStoreError::CorruptState {
            detail: format!("migration record is invalid: {error}"),
        })
    }
}
