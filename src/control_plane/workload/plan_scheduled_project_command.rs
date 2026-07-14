use super::{
    RuntimeEnvironment, RuntimeEnvironmentOptions, ScheduledProjectCommandPlan,
    ScheduledProjectCommandPlanOptions, WorkloadPlanError, merge_project_declared_environment,
};
use crate::control_plane::state::ManagedEnvironmentRecord;
use crate::control_plane::{ServiceDeploymentStrategy, ServiceExecutionPlan};
use std::time::Duration;

const SCHEDULED_COMMAND_TIMEOUT: Duration = Duration::from_secs(55);

/// Plans one minute-triggered command inside its exact application runtime.
pub(crate) fn plan_scheduled_project_command(
    service: &ServiceExecutionPlan,
    application: &ServiceExecutionPlan,
    managed_environment: ManagedEnvironmentRecord,
) -> Result<ScheduledProjectCommandPlan, WorkloadPlanError> {
    if service.strategy() != ServiceDeploymentStrategy::ProjectScheduledCommand {
        return Err(invalid(format!(
            "service '{}-{}' is not a scheduled project command",
            service.project().as_str(),
            service.service().as_str()
        )));
    }
    let declared =
        merge_project_declared_environment(service, application, "scheduled project command")?;
    let environment = RuntimeEnvironment::new(RuntimeEnvironmentOptions {
        project: service.project().clone(),
        declared,
        managed: managed_environment,
    })
    .map_err(invalid)?;
    let arguments = service
        .desired()
        .command()
        .map_or_else(default_command, <[String]>::to_vec);

    ScheduledProjectCommandPlan::new(ScheduledProjectCommandPlanOptions {
        project: service.project().clone(),
        service: service.service().clone(),
        application_service: application.service().clone(),
        arguments,
        environment: environment.values().clone(),
        timeout: SCHEDULED_COMMAND_TIMEOUT,
    })
    .map_err(invalid)
}

fn default_command() -> Vec<String> {
    ["php", "artisan", "schedule:run", "--no-interaction"]
        .map(str::to_owned)
        .to_vec()
}

fn invalid(error: impl std::fmt::Display) -> WorkloadPlanError {
    WorkloadPlanError::new(error.to_string())
}
