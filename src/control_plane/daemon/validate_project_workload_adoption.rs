use super::EngineReconciliationPlanError;
use crate::control_plane::engine::ResourceKind;
use crate::control_plane::state::{ResourceLifecycle, ResourceRecord};
use crate::control_plane::{ExecutionPlan, ServiceDeploymentStrategy};

/// Blocks implicit reactivation when a desired workload has only retained state.
pub(crate) fn validate_project_workload_adoption(
    execution: &ExecutionPlan,
    resources: &[ResourceRecord],
) -> Result<(), EngineReconciliationPlanError> {
    for service in execution.services() {
        let kind = match service.strategy() {
            ServiceDeploymentStrategy::ProjectApplication => ResourceKind::ProjectApplication,
            ServiceDeploymentStrategy::ProjectProcess => ResourceKind::ProjectProcess,
            ServiceDeploymentStrategy::DedicatedProject
            | ServiceDeploymentStrategy::DedicatedUntilIsolationProven => {
                ResourceKind::ProjectService
            }
            _ => continue,
        };
        let matching = resources
            .iter()
            .filter(|resource| {
                resource.kind() == kind.label()
                    && resource.project_id() == Some(service.project().as_str())
                    && resource.scope_id() == Some(service.service().as_str())
            })
            .collect::<Vec<_>>();
        if matching
            .iter()
            .any(|resource| resource.lifecycle() == ResourceLifecycle::Active)
        {
            continue;
        }
        let Some(retained) = matching.first() else {
            continue;
        };
        let lifecycle = match retained.lifecycle() {
            ResourceLifecycle::Active => unreachable!("active resources returned above"),
            ResourceLifecycle::Orphaned => "orphaned",
            ResourceLifecycle::Retained => "retained",
        };

        return Err(EngineReconciliationPlanError::new(format!(
            "project workload '{}-{}' is {lifecycle}; run explicit project adoption before reconciliation",
            service.project().as_str(),
            service.service().as_str()
        )));
    }

    Ok(())
}
