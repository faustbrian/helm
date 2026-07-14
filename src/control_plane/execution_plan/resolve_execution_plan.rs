use super::{ExecutionPlan, ServiceExecutionPlan};
use crate::control_plane::application::DesiredRegistry;
use crate::control_plane::{
    ServiceDeploymentStrategy, ServiceStrategyError, resolve_service_deployment_strategy,
};

/// Resolves a complete desired registry into deterministic backend strategies.
pub(crate) fn resolve_execution_plan(
    registry: &DesiredRegistry,
) -> Result<ExecutionPlan, ServiceStrategyError> {
    let mut services = Vec::new();

    for project in registry.projects() {
        for service_name in project.startup_order() {
            let service = project.service(service_name).ok_or_else(|| {
                ServiceStrategyError::invalid_plan(format!(
                    "project '{}' startup order references missing service '{service_name}'",
                    project.identity().as_str()
                ))
            })?;
            let strategy = service
                .preset()
                .map(resolve_service_deployment_strategy)
                .transpose()?
                .unwrap_or(ServiceDeploymentStrategy::ProjectApplication);
            services.push(ServiceExecutionPlan::new(
                project.identity().clone(),
                project.project_directory().to_path_buf(),
                service.identity().clone(),
                strategy,
                service.clone(),
            ));
        }
    }

    Ok(ExecutionPlan::new(services))
}
