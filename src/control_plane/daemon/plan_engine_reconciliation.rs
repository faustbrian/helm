use super::{
    EngineReconciliationPlan, EngineReconciliationPlanError, EngineReconciliationPlanOptions,
};
use crate::control_plane::ServiceDeploymentStrategy;
use crate::control_plane::gateway::GatewaySnapshot;
use crate::control_plane::state::{
    EnvironmentLifecycle, ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
};
use crate::control_plane::workload::{
    ImmutableProjectApplicationOptions, plan_immutable_project_application,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Resolves a whole registry before allowing any Engine mutation to begin.
pub(crate) fn plan_engine_reconciliation(
    options: EngineReconciliationPlanOptions<'_>,
) -> Result<EngineReconciliationPlan, EngineReconciliationPlanError> {
    let mut applications = Vec::new();
    let mut routes = options.shared_routes.to_vec();

    for service in options.execution.services() {
        let is_shared = matches!(
            service.strategy(),
            ServiceDeploymentStrategy::SharedByCompatibility
                | ServiceDeploymentStrategy::SharedWithAttribution
                | ServiceDeploymentStrategy::SharedStateless
        );
        if is_shared
            && options
                .prepared_shared_services
                .iter()
                .any(|(project, name)| {
                    project == service.project().as_str() && name == service.service().as_str()
                })
        {
            continue;
        }
        if service.strategy() != ServiceDeploymentStrategy::ProjectApplication {
            return Err(invalid(format!(
                "service '{}-{}' strategy {:?} has no registered Engine reconciler",
                service.project().as_str(),
                service.service().as_str(),
                service.strategy()
            )));
        }
        let managed_environment = options
            .managed_environments
            .iter()
            .find(|environment| environment.project_id() == service.project().as_str())
            .cloned()
            .unwrap_or_else(|| empty_environment(service.project().as_str()));
        let application = plan_immutable_project_application(ImmutableProjectApplicationOptions {
            service,
            managed_environment,
            installation_id: options.installation_id,
            schema_version: options.schema_version,
            platform: options.platform,
            network_name: options.network_name,
            internal_http_port: options.internal_http_port,
        })
        .map_err(invalid)?;
        routes.push(application.route().clone());
        applications.push(application);
    }

    let gateway = GatewaySnapshot::new(routes).map_err(invalid)?;

    Ok(EngineReconciliationPlan::new(applications, gateway))
}

fn empty_environment(project_id: &str) -> ManagedEnvironmentRecord {
    let mut digest = Sha256::new();
    digest.update(b"stackctl-empty-managed-environment-v1\0");
    digest.update(project_id.as_bytes());

    ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: project_id.to_owned(),
        revision: format!("sha256:{}", hex::encode(digest.finalize())),
        values: BTreeMap::new(),
        lifecycle: EnvironmentLifecycle::Active,
    })
}

fn invalid(error: impl std::fmt::Display) -> EngineReconciliationPlanError {
    EngineReconciliationPlanError::new(error.to_string())
}
