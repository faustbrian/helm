use super::persisted_migration::PersistedMigration;
use super::{MigrationPhase, MigrationRecord, StateStoreError};
use rusqlite::{OptionalExtension, Transaction, params};

/// Persists one monotonic migration checkpoint in the caller's transaction.
pub(super) fn persist_migration_record(
    transaction: &Transaction<'_>,
    migration: &MigrationRecord,
) -> Result<(), StateStoreError> {
    let existing = transaction
        .query_row(
            "SELECT migration_id, project_id, source_revision, target_revision,
                    source_compatibility_fingerprint,
                    target_compatibility_fingerprint, phase,
                    backup_reference, backup_artifact_sha256,
                    backup_artifact_size_bytes,
                    target_resource_id, rollback_reference,
                    updated_at_unix_seconds
             FROM migrations WHERE migration_id = ?1",
            [migration.migration_id()],
            |row| {
                Ok(PersistedMigration {
                    migration_id: row.get(0)?,
                    project_id: row.get(1)?,
                    source_revision: row.get(2)?,
                    target_revision: row.get(3)?,
                    source_compatibility_fingerprint: row.get(4)?,
                    target_compatibility_fingerprint: row.get(5)?,
                    phase: row.get(6)?,
                    backup_reference: row.get(7)?,
                    backup_artifact_sha256: row.get(8)?,
                    backup_artifact_size_bytes: row.get(9)?,
                    target_resource_id: row.get(10)?,
                    rollback_reference: row.get(11)?,
                    updated_at_unix_seconds: row.get(12)?,
                })
            },
        )
        .optional()?
        .map(PersistedMigration::into_record)
        .transpose()?;

    if let Some(existing) = existing {
        if existing == *migration {
            return Ok(());
        }
        if !migration.has_same_identity(&existing) {
            return Err(StateStoreError::MigrationIdentityConflict {
                migration_id: migration.migration_id().to_owned(),
            });
        }
        if !migration.preserves_evidence_from(&existing) {
            return Err(StateStoreError::MigrationEvidenceConflict {
                migration_id: migration.migration_id().to_owned(),
            });
        }
        if migration.updated_at_unix_seconds() < existing.updated_at_unix_seconds() {
            return Err(StateStoreError::MigrationTimeRegression {
                migration_id: migration.migration_id().to_owned(),
            });
        }
        if !existing.phase().can_advance_to(migration.phase()) {
            return Err(StateStoreError::InvalidMigrationTransition {
                migration_id: migration.migration_id().to_owned(),
                from: existing.phase().label().to_owned(),
                to: migration.phase().label().to_owned(),
            });
        }

        transaction.execute(
            "UPDATE migrations SET
                 phase = ?1,
                 backup_reference = ?2,
                 backup_artifact_sha256 = ?3,
                 backup_artifact_size_bytes = ?4,
                 target_resource_id = ?5,
                 rollback_reference = ?6,
                 updated_at_unix_seconds = ?7
             WHERE migration_id = ?8",
            params![
                migration.phase().label(),
                migration.backup_reference(),
                migration.backup_artifact_sha256(),
                migration
                    .backup_artifact_size_bytes()
                    .map(|size| size as i64),
                migration.target_resource_id(),
                migration.rollback_reference(),
                migration.updated_at_unix_seconds(),
                migration.migration_id(),
            ],
        )?;
    } else {
        if migration.phase() != MigrationPhase::Inventoried {
            return Err(StateStoreError::InvalidMigrationTransition {
                migration_id: migration.migration_id().to_owned(),
                from: "absent".to_owned(),
                to: migration.phase().label().to_owned(),
            });
        }
        transaction.execute(
            "INSERT INTO migrations (
                 migration_id, project_id, source_revision, target_revision,
                 source_compatibility_fingerprint,
                 target_compatibility_fingerprint, phase,
                 backup_reference, backup_artifact_sha256,
                 backup_artifact_size_bytes, target_resource_id,
                 rollback_reference, updated_at_unix_seconds
             ) VALUES (
                 ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13
             )",
            params![
                migration.migration_id(),
                migration.project_id(),
                migration.source_revision(),
                migration.target_revision(),
                migration.source_compatibility_fingerprint(),
                migration.target_compatibility_fingerprint(),
                migration.phase().label(),
                migration.backup_reference(),
                migration.backup_artifact_sha256(),
                migration
                    .backup_artifact_size_bytes()
                    .map(|size| size as i64),
                migration.target_resource_id(),
                migration.rollback_reference(),
                migration.updated_at_unix_seconds(),
            ],
        )?;
    }

    Ok(())
}
