use super::{
    MigrationError, MigrationExecutionResult, MigrationOperations, RecoveryPointRestoreOptions,
    execute_migration,
};
use crate::control_plane::state::{
    MigrationPhase, MigrationRecord, MigrationRecordOptions, RecoveryPointRecord, StateStore,
};

/// Restores one exact cataloged artifact through the reversible migration path.
pub(crate) async fn execute_recovery_point_restore(
    store: &mut dyn StateStore,
    inventory: &MigrationRecord,
    operations: &mut dyn MigrationOperations,
    options: &RecoveryPointRestoreOptions<'_>,
) -> Result<MigrationExecutionResult, MigrationError> {
    let recovery_point = select_recovery_point(store, inventory, options)?;
    prepare_checkpoint(
        store,
        inventory,
        &recovery_point,
        options.updated_at_unix_seconds,
    )?;

    execute_migration(
        store,
        inventory,
        operations,
        options.updated_at_unix_seconds,
    )
    .await
}

fn select_recovery_point(
    store: &dyn StateStore,
    inventory: &MigrationRecord,
    options: &RecoveryPointRestoreOptions<'_>,
) -> Result<RecoveryPointRecord, MigrationError> {
    let recovery_point = store
        .recovery_points(inventory.project_id())?
        .into_iter()
        .find(|point| point.recovery_point_id() == options.recovery_point_id)
        .ok_or_else(|| MigrationError::RecoveryPointNotFound {
            recovery_point_id: options.recovery_point_id.to_owned(),
            project_id: inventory.project_id().to_owned(),
        })?;
    let matches = recovery_point.project_id() == inventory.project_id()
        && recovery_point.service_id() == options.service_id
        && recovery_point.logical_resource_id() == options.logical_resource_id
        && recovery_point.resource_kind() == options.resource_kind
        && recovery_point.compatibility_fingerprint()
            == inventory.source_compatibility_fingerprint();
    if !matches {
        return Err(MigrationError::RecoveryPointMismatch {
            recovery_point_id: options.recovery_point_id.to_owned(),
            project_id: inventory.project_id().to_owned(),
            service_id: options.service_id.to_owned(),
            logical_resource_id: options.logical_resource_id.to_owned(),
        });
    }
    if options.updated_at_unix_seconds < recovery_point.verified_at_unix_seconds() {
        return Err(MigrationError::InvalidInventory {
            detail: "restore execution time predates recovery-point verification".to_owned(),
        });
    }

    Ok(recovery_point)
}

fn prepare_checkpoint(
    store: &mut dyn StateStore,
    inventory: &MigrationRecord,
    recovery_point: &RecoveryPointRecord,
    updated_at_unix_seconds: i64,
) -> Result<(), MigrationError> {
    super::run_migration::validate_inventory(inventory, updated_at_unix_seconds)?;
    let existing = super::run_migration::load_checkpoint(store, inventory)?;
    if let Some(checkpoint) = existing.as_ref()
        && checkpoint.phase() != MigrationPhase::Inventoried
    {
        let evidence_matches = checkpoint.backup_reference() == Some(recovery_point.reference())
            && checkpoint.backup_artifact_sha256() == Some(recovery_point.artifact_sha256())
            && checkpoint.backup_artifact_size_bytes()
                == Some(recovery_point.artifact_size_bytes());
        if !evidence_matches {
            return Err(MigrationError::CheckpointMismatch {
                migration_id: inventory.migration_id().to_owned(),
            });
        }

        return Ok(());
    }
    if existing.is_none() {
        store.record_migration(inventory)?;
    }

    let checkpoint = MigrationRecord::new(MigrationRecordOptions {
        migration_id: inventory.migration_id().to_owned(),
        project_id: inventory.project_id().to_owned(),
        source_revision: inventory.source_revision().to_owned(),
        target_revision: inventory.target_revision().to_owned(),
        source_compatibility_fingerprint: inventory.source_compatibility_fingerprint().to_owned(),
        target_compatibility_fingerprint: inventory.target_compatibility_fingerprint().to_owned(),
        phase: MigrationPhase::BackupVerified,
        backup_reference: Some(recovery_point.reference().to_owned()),
        backup_artifact_sha256: Some(recovery_point.artifact_sha256().to_owned()),
        backup_artifact_size_bytes: Some(recovery_point.artifact_size_bytes()),
        target_resource_id: None,
        rollback_reference: inventory.rollback_reference().map(str::to_owned),
        updated_at_unix_seconds,
    })
    .map_err(|error| MigrationError::InvalidCheckpoint {
        detail: error.to_string(),
    })?;
    store.record_migration(&checkpoint)?;

    Ok(())
}
