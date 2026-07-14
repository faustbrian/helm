use super::prepare_v7_migration::{executor, same_plan, validate_executor_set};
use super::{V7MigrationAdapterExecutor, V7MigrationExecutionError, V7MigrationExecutionJournal};
use crate::control_plane::state::{V7MigrationExecutionPhase, V7MigrationExecutionRecord};
use std::collections::BTreeMap;

/// Idempotently applies every prepared adapter and records one global cutover.
pub(crate) async fn cutover_v7_migration(
    journal: &mut dyn V7MigrationExecutionJournal,
    plan: &V7MigrationExecutionRecord,
    executors: &mut BTreeMap<String, Box<dyn V7MigrationAdapterExecutor>>,
    updated_at_unix_seconds: i64,
) -> Result<V7MigrationExecutionRecord, V7MigrationExecutionError> {
    let execution = load_execution(journal, plan)?;
    if execution.phase() == V7MigrationExecutionPhase::Cutover {
        return Ok(execution);
    }
    validate_executor_set(&execution, executors)?;
    validate_phase_and_time(
        &execution,
        V7MigrationExecutionPhase::Prepared,
        updated_at_unix_seconds,
        "cut over",
    )?;

    let mut indexes = (0..execution.checkpoints().len()).collect::<Vec<_>>();
    indexes.sort_by_key(|index| cutover_rank(execution.checkpoints()[*index].adapter_id()));
    for index in indexes {
        let checkpoint = &execution.checkpoints()[index];
        executor(executors, checkpoint)?
            .as_mut()
            .cutover(checkpoint)
            .await
            .map_err(|source| V7MigrationExecutionError::Operation {
                adapter_id: checkpoint.adapter_id().to_owned(),
                action: "cutover",
                source,
            })?;
    }

    let cutover = execution
        .transition_all(V7MigrationExecutionPhase::Cutover, updated_at_unix_seconds)
        .map_err(|detail| V7MigrationExecutionError::InvalidPlan { detail })?;
    journal.persist_v7_execution(&cutover)?;

    Ok(cutover)
}

pub(super) fn load_execution(
    journal: &dyn V7MigrationExecutionJournal,
    plan: &V7MigrationExecutionRecord,
) -> Result<V7MigrationExecutionRecord, V7MigrationExecutionError> {
    let execution = journal
        .load_v7_execution(plan.canonical_project_path(), plan.evidence_revision())?
        .ok_or_else(|| V7MigrationExecutionError::InvalidPlan {
            detail: "prepared execution checkpoint is missing".to_owned(),
        })?;
    if !same_plan(&execution, plan) {
        return Err(V7MigrationExecutionError::CheckpointMismatch);
    }

    Ok(execution)
}

pub(super) fn validate_phase_and_time(
    execution: &V7MigrationExecutionRecord,
    expected: V7MigrationExecutionPhase,
    updated_at_unix_seconds: i64,
    action: &str,
) -> Result<(), V7MigrationExecutionError> {
    if execution.phase() != expected {
        return Err(V7MigrationExecutionError::InvalidPlan {
            detail: format!("cannot {action} from phase '{}'", execution.phase().label()),
        });
    }
    if updated_at_unix_seconds < execution.updated_at_unix_seconds() {
        return Err(V7MigrationExecutionError::InvalidPlan {
            detail: format!("{action} time predates durable execution"),
        });
    }

    Ok(())
}

pub(super) fn cutover_rank(adapter_id: &str) -> u8 {
    if adapter_id == "route" {
        4
    } else if adapter_id == "environment" {
        3
    } else if adapter_id == "trust" {
        0
    } else if adapter_id.starts_with("service/") {
        2
    } else {
        1
    }
}
