use super::prepare_v7_migration::same_plan;
use super::{V7MigrationCutoverOptions, V7MigrationExecutionError, V7MigrationExecutionJournal};
use crate::control_plane::state::{V7MigrationExecutionPhase, V7MigrationExecutionRecord};

/// Idempotently applies every prepared adapter and records one global cutover.
pub(crate) async fn cutover_v7_migration(
    options: V7MigrationCutoverOptions<'_, '_>,
) -> Result<V7MigrationExecutionRecord, V7MigrationExecutionError> {
    let execution = load_execution(options.journal, options.plan)?;
    if options.desired_state.project().project_name() != execution.project_id()
        || options.desired_state.environment().project_id() != execution.project_id()
        || options.desired_state.project().canonical_path() != execution.canonical_project_path()
    {
        return Err(V7MigrationExecutionError::InvalidPlan {
            detail: "desired project state does not match the immutable execution identity"
                .to_owned(),
        });
    }
    if execution.phase() == V7MigrationExecutionPhase::Cutover {
        return Ok(execution);
    }
    options.registry.validate(&execution)?;
    validate_phase_and_time(
        &execution,
        V7MigrationExecutionPhase::Prepared,
        options.updated_at_unix_seconds,
        "cut over",
    )?;

    let mut indexes = (0..execution.checkpoints().len()).collect::<Vec<_>>();
    indexes.sort_by_key(|index| cutover_rank(execution.checkpoints()[*index].adapter_id()));
    for index in indexes {
        let checkpoint = &execution.checkpoints()[index];
        options
            .registry
            .executor(checkpoint)?
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
        .transition_all(
            V7MigrationExecutionPhase::Cutover,
            options.updated_at_unix_seconds,
        )
        .map_err(|detail| V7MigrationExecutionError::InvalidPlan { detail })?;
    options
        .journal
        .persist_v7_cutover(options.desired_state, &cutover)?;

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
