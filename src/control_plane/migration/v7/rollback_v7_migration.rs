use super::cutover_v7_migration::{cutover_rank, load_execution};
use super::prepare_v7_migration::{executor, validate_executor_set};
use super::{V7MigrationAdapterExecutor, V7MigrationExecutionError, V7MigrationExecutionJournal};
use crate::control_plane::state::{V7MigrationExecutionPhase, V7MigrationExecutionRecord};
use std::collections::BTreeMap;

/// Restores every source in reverse adapter order before journaling rollback.
pub(crate) async fn rollback_v7_migration(
    journal: &mut dyn V7MigrationExecutionJournal,
    plan: &V7MigrationExecutionRecord,
    executors: &mut BTreeMap<String, Box<dyn V7MigrationAdapterExecutor>>,
    updated_at_unix_seconds: i64,
) -> Result<V7MigrationExecutionRecord, V7MigrationExecutionError> {
    let execution = load_execution(journal, plan)?;
    if execution.phase() == V7MigrationExecutionPhase::RolledBack {
        return Ok(execution);
    }
    if execution.phase() == V7MigrationExecutionPhase::Confirmed {
        return Err(V7MigrationExecutionError::InvalidPlan {
            detail: "cannot roll back after source retirement".to_owned(),
        });
    }
    validate_executor_set(&execution, executors)?;
    if updated_at_unix_seconds < execution.updated_at_unix_seconds() {
        return Err(V7MigrationExecutionError::InvalidPlan {
            detail: "roll back time predates durable execution".to_owned(),
        });
    }

    let mut indexes = (0..execution.checkpoints().len()).collect::<Vec<_>>();
    indexes.sort_by_key(|index| {
        std::cmp::Reverse(cutover_rank(execution.checkpoints()[*index].adapter_id()))
    });
    for index in indexes {
        let checkpoint = &execution.checkpoints()[index];
        executor(executors, checkpoint)?
            .as_mut()
            .rollback(checkpoint)
            .await
            .map_err(|source| V7MigrationExecutionError::Operation {
                adapter_id: checkpoint.adapter_id().to_owned(),
                action: "rollback",
                source,
            })?;
    }
    let rolled_back = execution
        .transition_all(
            V7MigrationExecutionPhase::RolledBack,
            updated_at_unix_seconds,
        )
        .map_err(|detail| V7MigrationExecutionError::InvalidPlan { detail })?;
    journal.persist_v7_execution(&rolled_back)?;

    Ok(rolled_back)
}
