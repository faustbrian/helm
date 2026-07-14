use super::{V7MigrationAdapterExecutor, V7MigrationExecutionError, V7MigrationExecutionJournal};
use crate::control_plane::state::{
    V7MigrationAdapterCheckpointPhase, V7MigrationExecutionPhase, V7MigrationExecutionRecord,
};
use std::collections::BTreeMap;

/// Prepares every selected adapter and stops at the durable all-target barrier.
pub(crate) async fn prepare_v7_migration(
    journal: &mut dyn V7MigrationExecutionJournal,
    plan: &V7MigrationExecutionRecord,
    executors: &mut BTreeMap<String, Box<dyn V7MigrationAdapterExecutor>>,
    updated_at_unix_seconds: i64,
) -> Result<V7MigrationExecutionRecord, V7MigrationExecutionError> {
    validate_plan(plan, executors, updated_at_unix_seconds)?;
    let mut execution =
        match journal.load_v7_execution(plan.canonical_project_path(), plan.evidence_revision())? {
            Some(execution) => {
                if !same_plan(&execution, plan) {
                    return Err(V7MigrationExecutionError::CheckpointMismatch);
                }
                execution
            }
            None => {
                journal.persist_v7_execution(plan)?;
                plan.clone()
            }
        };
    if execution.phase() == V7MigrationExecutionPhase::Prepared {
        return Ok(execution);
    }
    if !matches!(
        execution.phase(),
        V7MigrationExecutionPhase::Planned | V7MigrationExecutionPhase::Preparing
    ) {
        return Err(V7MigrationExecutionError::InvalidPlan {
            detail: format!("cannot prepare from phase '{}'", execution.phase().label()),
        });
    }

    for index in 0..execution.checkpoints().len() {
        let checkpoint = execution.checkpoints()[index].clone();
        if checkpoint.phase() == V7MigrationAdapterCheckpointPhase::Pending
            && checkpoint.requires_recovery()
        {
            let backup = executor(executors, &checkpoint)?
                .as_mut()
                .prepare_recovery(&checkpoint)
                .await
                .map_err(|source| V7MigrationExecutionError::Operation {
                    adapter_id: checkpoint.adapter_id().to_owned(),
                    action: "recovery preparation",
                    source,
                })?;
            let replacement = checkpoint.with_recovery_verified(
                backup.reference(),
                backup.artifact_sha256(),
                backup.artifact_size_bytes(),
                updated_at_unix_seconds,
            )?;
            execution = execution.with_checkpoint(
                replacement,
                V7MigrationExecutionPhase::Preparing,
                updated_at_unix_seconds,
            )?;
            journal.persist_v7_execution(&execution)?;
        }
    }

    let mut target_indexes = (0..execution.checkpoints().len()).collect::<Vec<_>>();
    target_indexes.sort_by_key(|index| !execution.checkpoints()[*index].requires_recovery());
    for index in target_indexes {
        let checkpoint = execution.checkpoints()[index].clone();
        if matches!(
            checkpoint.phase(),
            V7MigrationAdapterCheckpointPhase::Pending
                | V7MigrationAdapterCheckpointPhase::RecoveryVerified
        ) {
            let target = executor(executors, &checkpoint)?
                .as_mut()
                .prepare_target(&checkpoint)
                .await
                .map_err(|source| V7MigrationExecutionError::Operation {
                    adapter_id: checkpoint.adapter_id().to_owned(),
                    action: "target preparation",
                    source,
                })?;
            let replacement =
                checkpoint.with_target_verified(target.reference(), updated_at_unix_seconds)?;
            execution = execution.with_checkpoint(
                replacement,
                V7MigrationExecutionPhase::Preparing,
                updated_at_unix_seconds,
            )?;
            journal.persist_v7_execution(&execution)?;
        }
    }

    execution = execution
        .with_phase(V7MigrationExecutionPhase::Prepared, updated_at_unix_seconds)
        .map_err(|detail| V7MigrationExecutionError::InvalidPlan { detail })?;
    journal.persist_v7_execution(&execution)?;

    Ok(execution)
}

fn validate_plan(
    plan: &V7MigrationExecutionRecord,
    executors: &BTreeMap<String, Box<dyn V7MigrationAdapterExecutor>>,
    updated_at_unix_seconds: i64,
) -> Result<(), V7MigrationExecutionError> {
    if plan.phase() != V7MigrationExecutionPhase::Planned {
        return Err(V7MigrationExecutionError::InvalidPlan {
            detail: "initial phase must be 'planned'".to_owned(),
        });
    }
    if updated_at_unix_seconds < plan.updated_at_unix_seconds() {
        return Err(V7MigrationExecutionError::InvalidPlan {
            detail: "preparation time predates the selected plan".to_owned(),
        });
    }
    validate_executor_set(plan, executors)?;

    Ok(())
}

pub(super) fn validate_executor_set(
    execution: &V7MigrationExecutionRecord,
    executors: &BTreeMap<String, Box<dyn V7MigrationAdapterExecutor>>,
) -> Result<(), V7MigrationExecutionError> {
    for checkpoint in execution.checkpoints() {
        if !executors.contains_key(checkpoint.adapter_kind()) {
            return Err(V7MigrationExecutionError::MissingAdapter {
                adapter_kind: checkpoint.adapter_kind().to_owned(),
            });
        }
    }

    Ok(())
}

pub(super) fn same_plan(
    execution: &V7MigrationExecutionRecord,
    plan: &V7MigrationExecutionRecord,
) -> bool {
    execution.project_id() == plan.project_id()
        && execution.canonical_project_path() == plan.canonical_project_path()
        && execution.evidence_revision() == plan.evidence_revision()
        && execution.adapter_plan_revision() == plan.adapter_plan_revision()
        && execution.checkpoints().len() == plan.checkpoints().len()
        && execution
            .checkpoints()
            .iter()
            .zip(plan.checkpoints())
            .all(|(execution, plan)| {
                execution.adapter_id() == plan.adapter_id()
                    && execution.adapter_kind() == plan.adapter_kind()
                    && execution.requires_recovery() == plan.requires_recovery()
            })
}

pub(super) fn executor<'registry>(
    executors: &'registry mut BTreeMap<String, Box<dyn V7MigrationAdapterExecutor>>,
    checkpoint: &crate::control_plane::state::V7MigrationAdapterCheckpoint,
) -> Result<&'registry mut Box<dyn V7MigrationAdapterExecutor>, V7MigrationExecutionError> {
    executors.get_mut(checkpoint.adapter_kind()).ok_or_else(|| {
        V7MigrationExecutionError::MissingAdapter {
            adapter_kind: checkpoint.adapter_kind().to_owned(),
        }
    })
}

impl From<String> for V7MigrationExecutionError {
    fn from(detail: String) -> Self {
        Self::InvalidPlan { detail }
    }
}
