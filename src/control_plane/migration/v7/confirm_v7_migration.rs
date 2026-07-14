use super::cutover_v7_migration::{load_execution, validate_phase_and_time};
use super::prepare_v7_migration::{executor, validate_executor_set};
use super::{V7MigrationAdapterExecutor, V7MigrationExecutionError, V7MigrationExecutionJournal};
use crate::control_plane::state::{V7MigrationExecutionPhase, V7MigrationExecutionRecord};
use std::collections::BTreeMap;

/// Retires every retained source only from an explicit cutover checkpoint.
pub(crate) async fn confirm_v7_migration(
    journal: &mut dyn V7MigrationExecutionJournal,
    plan: &V7MigrationExecutionRecord,
    executors: &mut BTreeMap<String, Box<dyn V7MigrationAdapterExecutor>>,
    updated_at_unix_seconds: i64,
) -> Result<V7MigrationExecutionRecord, V7MigrationExecutionError> {
    let execution = load_execution(journal, plan)?;
    if execution.phase() == V7MigrationExecutionPhase::Confirmed {
        return Ok(execution);
    }
    validate_executor_set(&execution, executors)?;
    validate_phase_and_time(
        &execution,
        V7MigrationExecutionPhase::Cutover,
        updated_at_unix_seconds,
        "confirm",
    )?;

    for checkpoint in execution.checkpoints() {
        executor(executors, checkpoint)?
            .as_mut()
            .confirm(checkpoint)
            .await
            .map_err(|source| V7MigrationExecutionError::Operation {
                adapter_id: checkpoint.adapter_id().to_owned(),
                action: "confirmation",
                source,
            })?;
    }
    let confirmed = execution
        .transition_all(
            V7MigrationExecutionPhase::Confirmed,
            updated_at_unix_seconds,
        )
        .map_err(|detail| V7MigrationExecutionError::InvalidPlan { detail })?;
    journal.persist_v7_execution(&confirmed)?;

    Ok(confirmed)
}
