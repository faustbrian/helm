use super::cutover_v7_migration::{cutover_rank, load_execution};
use super::{V7MigrationExecutionError, V7MigrationRollbackOptions};
use crate::control_plane::state::{V7MigrationExecutionPhase, V7MigrationExecutionRecord};

/// Restores every source in reverse adapter order before journaling rollback.
pub(crate) async fn rollback_v7_migration(
    options: V7MigrationRollbackOptions<'_>,
) -> Result<V7MigrationExecutionRecord, V7MigrationExecutionError> {
    let execution = load_execution(options.journal, options.plan)?;
    if options.restored_state.project().project_name() != execution.project_id()
        || options.restored_state.environment().project_id() != execution.project_id()
        || options.restored_state.project().canonical_path() != execution.canonical_project_path()
        || options
            .restored_state
            .retained_targets()
            .iter()
            .any(|target| target.project_id() != execution.project_id())
    {
        return Err(V7MigrationExecutionError::InvalidPlan {
            detail: "restored project state does not match the immutable execution identity"
                .to_owned(),
        });
    }
    if execution.phase() == V7MigrationExecutionPhase::RolledBack {
        return Ok(execution);
    }
    if execution.phase() == V7MigrationExecutionPhase::Confirmed {
        return Err(V7MigrationExecutionError::InvalidPlan {
            detail: "cannot roll back after source retirement".to_owned(),
        });
    }
    options.registry.validate(&execution)?;
    if options.updated_at_unix_seconds < execution.updated_at_unix_seconds() {
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
        options
            .registry
            .executor(checkpoint)?
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
            options.updated_at_unix_seconds,
        )
        .map_err(|detail| V7MigrationExecutionError::InvalidPlan { detail })?;
    options
        .journal
        .persist_v7_rollback(options.restored_state, &rolled_back)?;

    Ok(rolled_back)
}
