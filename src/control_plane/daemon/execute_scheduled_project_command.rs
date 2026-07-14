use super::execute_queued_project_command::find_application;
use crate::control_plane::engine::{
    AttachedCommandOutput, CommandExecutor, ContainerDiscovery, EngineError,
};
use crate::control_plane::workload::{ScheduledProjectCommandPlan, run_project_command};

/// Executes one daemon schedule only in its exact owned application runtime.
pub(crate) async fn execute_scheduled_project_command<E>(
    engine: E,
    plan: ScheduledProjectCommandPlan,
    installation_id: String,
    schema_version: u32,
) -> Result<AttachedCommandOutput, EngineError>
where
    E: ContainerDiscovery + CommandExecutor,
{
    let application = find_application(
        &engine,
        plan.project_id(),
        plan.application_service(),
        &installation_id,
        schema_version,
    )
    .await?;
    let command = plan.command_plan()?;

    run_project_command(&engine, &application, &command).await
}
