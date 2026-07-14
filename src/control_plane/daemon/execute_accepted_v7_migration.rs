use super::{AcceptedV7MigrationAction, ExecuteAcceptedV7MigrationOptions};
use crate::control_plane::migration::{
    V7MigrationCutoverOptions, V7MigrationExecutionError, V7MigrationRollbackOptions,
    confirm_v7_migration, cutover_v7_migration, prepare_v7_migration, rollback_v7_migration,
};
use crate::control_plane::state::{V7MigrationExecutionPhase, V7MigrationExecutionRecord};

/// Replays one daemon-owned project-wide phase from its durable barrier.
pub(crate) async fn execute_accepted_v7_migration(
    options: ExecuteAcceptedV7MigrationOptions<'_, '_>,
) -> Result<V7MigrationExecutionRecord, V7MigrationExecutionError> {
    let durable = options
        .journal
        .load_v7_execution(
            options.plan.canonical_project_path(),
            options.plan.evidence_revision(),
        )?
        .ok_or_else(|| invalid("accepted v7 execution disappeared before dispatch"))?;
    if durable.project_id() != options.plan.project_id()
        || durable.adapter_plan_revision() != options.plan.adapter_plan_revision()
    {
        return Err(V7MigrationExecutionError::CheckpointMismatch);
    }

    match options.action {
        AcceptedV7MigrationAction::PrepareAndCutover { desired_state } => {
            match durable.phase() {
                V7MigrationExecutionPhase::Planned | V7MigrationExecutionPhase::Preparing => {
                    prepare_v7_migration(
                        options.journal,
                        options.plan,
                        options.registry,
                        options.updated_at_unix_seconds,
                    )
                    .await?;
                }
                V7MigrationExecutionPhase::Prepared => {}
                V7MigrationExecutionPhase::Cutover => return Ok(durable),
                V7MigrationExecutionPhase::Confirmed | V7MigrationExecutionPhase::RolledBack => {
                    return Err(invalid(format!(
                        "cannot prepare and cut over from phase '{}'",
                        durable.phase().label()
                    )));
                }
            }
            cutover_v7_migration(V7MigrationCutoverOptions {
                journal: options.journal,
                plan: options.plan,
                registry: options.registry,
                desired_state,
                updated_at_unix_seconds: options.updated_at_unix_seconds,
            })
            .await
        }
        AcceptedV7MigrationAction::Confirm => {
            confirm_v7_migration(
                options.journal,
                options.plan,
                options.registry,
                options.updated_at_unix_seconds,
            )
            .await
        }
        AcceptedV7MigrationAction::Rollback { restored_state } => {
            if durable.phase() == V7MigrationExecutionPhase::Confirmed {
                return Err(invalid("cannot roll back after source retirement"));
            }
            if !matches!(
                durable.phase(),
                V7MigrationExecutionPhase::Cutover | V7MigrationExecutionPhase::RolledBack
            ) {
                return Err(invalid(format!(
                    "cannot roll back project-wide migration from phase '{}'",
                    durable.phase().label()
                )));
            }
            rollback_v7_migration(V7MigrationRollbackOptions {
                journal: options.journal,
                plan: options.plan,
                registry: options.registry,
                restored_state,
                updated_at_unix_seconds: options.updated_at_unix_seconds,
            })
            .await
        }
    }
}

fn invalid(detail: impl Into<String>) -> V7MigrationExecutionError {
    V7MigrationExecutionError::InvalidPlan {
        detail: detail.into(),
    }
}
