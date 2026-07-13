use super::{MigrationError, MigrationExecutionResult, MigrationOperations};
use crate::control_plane::state::{
    MigrationPhase, MigrationRecord, MigrationRecordOptions, StateStore,
};

/// Resumes an inventoried migration and stops after reversible cutover.
pub(crate) async fn execute_migration(
    store: &mut dyn StateStore,
    inventory: &MigrationRecord,
    operations: &mut dyn MigrationOperations,
    updated_at_unix_seconds: i64,
) -> Result<MigrationExecutionResult, MigrationError> {
    validate_inventory(inventory, updated_at_unix_seconds)?;
    let mut checkpoint = match load_checkpoint(store, inventory)? {
        Some(checkpoint) => checkpoint,
        None => {
            store.record_migration(inventory)?;
            inventory.clone()
        }
    };

    loop {
        checkpoint = match checkpoint.phase() {
            MigrationPhase::Inventoried => {
                let backup = operations
                    .backup(&checkpoint)
                    .await
                    .map_err(|source| operation_error("backup", source))?;
                let mut options = next_options(
                    inventory,
                    &checkpoint,
                    MigrationPhase::BackupVerified,
                    updated_at_unix_seconds,
                );
                options.backup_reference = Some(backup.reference().to_owned());
                options.backup_artifact_sha256 = Some(backup.artifact_sha256().to_owned());
                options.backup_artifact_size_bytes = Some(backup.artifact_size_bytes());
                persist(store, options)?
            }
            MigrationPhase::BackupVerified => {
                let target_resource_id = operations
                    .provision_target(&checkpoint)
                    .await
                    .map_err(|source| operation_error("target provisioning", source))?;
                if target_resource_id.is_empty() {
                    return Err(MigrationError::InvalidCheckpoint {
                        detail: "target provisioning returned an empty resource identity"
                            .to_owned(),
                    });
                }
                let mut options = next_options(
                    inventory,
                    &checkpoint,
                    MigrationPhase::TargetProvisioned,
                    updated_at_unix_seconds,
                );
                options.target_resource_id = Some(target_resource_id);
                persist(store, options)?
            }
            MigrationPhase::TargetProvisioned => {
                let backup_reference = required(
                    checkpoint.backup_reference(),
                    "target checkpoint has no backup reference",
                )?;
                let target_resource_id = required(
                    checkpoint.target_resource_id(),
                    "target checkpoint has no resource identity",
                )?;
                operations
                    .restore(&checkpoint, &backup_reference, &target_resource_id)
                    .await
                    .map_err(|source| operation_error("restore", source))?;
                persist(
                    store,
                    next_options(
                        inventory,
                        &checkpoint,
                        MigrationPhase::DataRestored,
                        updated_at_unix_seconds,
                    ),
                )?
            }
            MigrationPhase::DataRestored => {
                let target_resource_id = required(
                    checkpoint.target_resource_id(),
                    "restored checkpoint has no resource identity",
                )?;
                operations
                    .verify_target(&checkpoint, &target_resource_id)
                    .await
                    .map_err(|source| operation_error("target verification", source))?;
                persist(
                    store,
                    next_options(
                        inventory,
                        &checkpoint,
                        MigrationPhase::TargetVerified,
                        updated_at_unix_seconds,
                    ),
                )?
            }
            MigrationPhase::TargetVerified => {
                let target_resource_id = required(
                    checkpoint.target_resource_id(),
                    "verified checkpoint has no resource identity",
                )?;
                let rollback_reference = required(
                    checkpoint.rollback_reference(),
                    "verified checkpoint has no rollback reference",
                )?;
                let cutover = operations
                    .plan_cutover(&checkpoint, &target_resource_id, &rollback_reference)
                    .await
                    .map_err(|source| operation_error("cutover", source))?;
                let cutover_checkpoint = checkpoint_from_options(next_options(
                    inventory,
                    &checkpoint,
                    MigrationPhase::Cutover,
                    updated_at_unix_seconds,
                ))?;
                store.record_migration_cutover(
                    cutover.project(),
                    cutover.environment(),
                    &cutover_checkpoint,
                )?;
                cutover_checkpoint
            }
            MigrationPhase::Cutover => {
                return Ok(MigrationExecutionResult::AwaitingConfirmation);
            }
            MigrationPhase::Confirmed => return Ok(MigrationExecutionResult::Confirmed),
            MigrationPhase::RolledBack => return Ok(MigrationExecutionResult::RolledBack),
        };
    }
}

/// Retires the retained source only after explicit confirmation.
pub(crate) async fn confirm_migration(
    store: &mut dyn StateStore,
    inventory: &MigrationRecord,
    operations: &mut dyn MigrationOperations,
    updated_at_unix_seconds: i64,
) -> Result<MigrationExecutionResult, MigrationError> {
    validate_inventory(inventory, updated_at_unix_seconds)?;
    let checkpoint =
        load_checkpoint(store, inventory)?.ok_or_else(|| MigrationError::MissingCheckpoint {
            migration_id: inventory.migration_id().to_owned(),
        })?;
    if checkpoint.phase() == MigrationPhase::Confirmed {
        return Ok(MigrationExecutionResult::Confirmed);
    }
    if checkpoint.phase() != MigrationPhase::Cutover {
        return Err(MigrationError::InvalidAction {
            migration_id: inventory.migration_id().to_owned(),
            detail: format!("cannot be confirmed from phase '{}'", checkpoint.phase()),
        });
    }

    operations
        .retire_source(inventory, &checkpoint)
        .await
        .map_err(|source| operation_error("source retirement", source))?;
    persist(
        store,
        next_options(
            inventory,
            &checkpoint,
            MigrationPhase::Confirmed,
            updated_at_unix_seconds,
        ),
    )?;

    Ok(MigrationExecutionResult::Confirmed)
}

/// Reverses any non-confirmed checkpoint and retains all recovery proof.
pub(crate) async fn rollback_migration(
    store: &mut dyn StateStore,
    inventory: &MigrationRecord,
    operations: &mut dyn MigrationOperations,
    updated_at_unix_seconds: i64,
) -> Result<MigrationExecutionResult, MigrationError> {
    validate_inventory(inventory, updated_at_unix_seconds)?;
    let checkpoint =
        load_checkpoint(store, inventory)?.ok_or_else(|| MigrationError::MissingCheckpoint {
            migration_id: inventory.migration_id().to_owned(),
        })?;
    if checkpoint.phase() == MigrationPhase::RolledBack {
        return Ok(MigrationExecutionResult::RolledBack);
    }
    if checkpoint.phase() == MigrationPhase::Confirmed {
        return Err(MigrationError::InvalidAction {
            migration_id: inventory.migration_id().to_owned(),
            detail: "cannot be rolled back after source retirement".to_owned(),
        });
    }

    let rollback = operations
        .plan_rollback(inventory, &checkpoint)
        .await
        .map_err(|source| operation_error("rollback", source))?;
    let rollback_checkpoint = checkpoint_from_options(next_options(
        inventory,
        &checkpoint,
        MigrationPhase::RolledBack,
        updated_at_unix_seconds,
    ))?;
    store.record_migration_rollback(
        rollback.project(),
        rollback.environment(),
        rollback.retained_targets(),
        &rollback_checkpoint,
    )?;

    Ok(MigrationExecutionResult::RolledBack)
}

fn validate_inventory(
    inventory: &MigrationRecord,
    updated_at_unix_seconds: i64,
) -> Result<(), MigrationError> {
    if inventory.phase() != MigrationPhase::Inventoried {
        return Err(MigrationError::InvalidInventory {
            detail: "initial phase must be 'inventoried'".to_owned(),
        });
    }
    if inventory.rollback_reference().is_none() {
        return Err(MigrationError::InvalidInventory {
            detail: "retained v7 rollback material is required".to_owned(),
        });
    }
    if updated_at_unix_seconds < inventory.updated_at_unix_seconds() {
        return Err(MigrationError::InvalidInventory {
            detail: "execution time predates inventory".to_owned(),
        });
    }

    Ok(())
}

fn load_checkpoint(
    store: &dyn StateStore,
    inventory: &MigrationRecord,
) -> Result<Option<MigrationRecord>, MigrationError> {
    let checkpoint = store
        .migrations()?
        .into_iter()
        .find(|record| record.migration_id() == inventory.migration_id());
    if checkpoint.as_ref().is_some_and(|checkpoint| {
        !checkpoint.has_same_identity(inventory)
            || checkpoint.rollback_reference() != inventory.rollback_reference()
    }) {
        return Err(MigrationError::CheckpointMismatch {
            migration_id: inventory.migration_id().to_owned(),
        });
    }

    Ok(checkpoint)
}

fn next_options(
    inventory: &MigrationRecord,
    checkpoint: &MigrationRecord,
    phase: MigrationPhase,
    updated_at_unix_seconds: i64,
) -> MigrationRecordOptions {
    MigrationRecordOptions {
        migration_id: inventory.migration_id().to_owned(),
        project_id: inventory.project_id().to_owned(),
        source_revision: inventory.source_revision().to_owned(),
        target_revision: inventory.target_revision().to_owned(),
        source_compatibility_fingerprint: inventory.source_compatibility_fingerprint().to_owned(),
        target_compatibility_fingerprint: inventory.target_compatibility_fingerprint().to_owned(),
        phase,
        backup_reference: checkpoint.backup_reference().map(str::to_owned),
        backup_artifact_sha256: checkpoint.backup_artifact_sha256().map(str::to_owned),
        backup_artifact_size_bytes: checkpoint.backup_artifact_size_bytes(),
        target_resource_id: checkpoint.target_resource_id().map(str::to_owned),
        rollback_reference: checkpoint.rollback_reference().map(str::to_owned),
        updated_at_unix_seconds,
    }
}

fn persist(
    store: &mut dyn StateStore,
    options: MigrationRecordOptions,
) -> Result<MigrationRecord, MigrationError> {
    let checkpoint = checkpoint_from_options(options)?;
    store.record_migration(&checkpoint)?;

    Ok(checkpoint)
}

fn checkpoint_from_options(
    options: MigrationRecordOptions,
) -> Result<MigrationRecord, MigrationError> {
    MigrationRecord::new(options).map_err(|error| MigrationError::InvalidCheckpoint {
        detail: error.to_string(),
    })
}

fn required(value: Option<&str>, detail: &str) -> Result<String, MigrationError> {
    value
        .map(str::to_owned)
        .ok_or_else(|| MigrationError::InvalidCheckpoint {
            detail: detail.to_owned(),
        })
}

fn operation_error(action: &'static str, source: super::MigrationOperationError) -> MigrationError {
    MigrationError::Operation { action, source }
}
