use super::{
    EngineReconciliationPlan, EngineReconciliationPlanError, EngineReconciliationPlanOptions,
};
use crate::control_plane::ServiceDeploymentStrategy;
use crate::control_plane::gateway::GatewaySnapshot;
use crate::control_plane::project_infrastructure::ProjectServicePreparationStrategy;
use crate::control_plane::state::{
    EnvironmentLifecycle, ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
};
use crate::control_plane::workload::{
    DedicatedProjectServiceOptions, ImmutableProjectApplicationOptions,
    ProjectProcessOperationOptions, plan_dedicated_project_service,
    plan_immutable_project_application, plan_project_process_operation,
    plan_scheduled_project_command,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Resolves a whole registry before allowing any Engine mutation to begin.
pub(crate) fn plan_engine_reconciliation(
    options: EngineReconciliationPlanOptions<'_>,
) -> Result<EngineReconciliationPlan, EngineReconciliationPlanError> {
    super::validate_project_workload_adoption(options.execution, options.durable_resources)?;
    let mut applications = Vec::new();
    let mut dedicated_services = Vec::new();
    let mut process_services = Vec::new();
    let mut scheduled_services = Vec::new();
    let mut routes = options.shared_routes.to_vec();
    routes.extend(
        options
            .prepared_project_services
            .iter()
            .filter_map(|prepared| prepared.route().cloned()),
    );

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
        if service.strategy() == ServiceDeploymentStrategy::Ephemeral {
            continue;
        }
        if service.strategy() == ServiceDeploymentStrategy::ProjectProcess {
            process_services.push(service);
            continue;
        }
        if service.strategy() == ServiceDeploymentStrategy::ProjectScheduledCommand {
            scheduled_services.push(service);
            continue;
        }
        if matches!(
            service.strategy(),
            ServiceDeploymentStrategy::DedicatedProject
                | ServiceDeploymentStrategy::DedicatedUntilIsolationProven
                | ServiceDeploymentStrategy::DedicatedRoutableProject
        ) {
            let prepared = options.prepared_project_services.iter().find(|prepared| {
                prepared.project_id() == service.project().as_str()
                    && prepared.service_id() == service.service().as_str()
            });
            let requires_preparation =
                ProjectServicePreparationStrategy::requires_preparation(service);
            if requires_preparation && prepared.is_none() {
                return Err(invalid(format!(
                    "managed project service '{}-{}' was not prepared",
                    service.project().as_str(),
                    service.service().as_str()
                )));
            }
            let generated_environment = prepared.map(|prepared| prepared.container_environment());
            dedicated_services.push(
                plan_dedicated_project_service(DedicatedProjectServiceOptions {
                    service,
                    generated_environment,
                    installation_id: options.installation_id,
                    schema_version: options.schema_version,
                    platform: options.platform,
                    network_name: options.network_name,
                })
                .map_err(invalid)?,
            );
            continue;
        }
        if service.strategy() != ServiceDeploymentStrategy::ProjectApplication {
            return Err(unsupported(service));
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

    let mut processes = Vec::with_capacity(process_services.len());
    for service in process_services {
        let application_service = resolve_application_dependency(
            options.execution.services(),
            service,
            "project process",
        )?;
        let application = resolve_planned_application(
            &applications,
            service,
            application_service,
            "project process",
        )?;
        let managed_environment = options
            .managed_environments
            .iter()
            .find(|environment| environment.project_id() == service.project().as_str())
            .cloned()
            .unwrap_or_else(|| empty_environment(service.project().as_str()));
        processes.push(
            plan_project_process_operation(ProjectProcessOperationOptions {
                service,
                application_service,
                application,
                managed_environment,
                installation_id: options.installation_id,
                schema_version: options.schema_version,
                platform: options.platform,
                network_name: options.network_name,
            })
            .map_err(invalid)?,
        );
    }

    let mut scheduled_commands = Vec::with_capacity(scheduled_services.len());
    for service in scheduled_services {
        let application_service = resolve_application_dependency(
            options.execution.services(),
            service,
            "scheduled project command",
        )?;
        resolve_planned_application(
            &applications,
            service,
            application_service,
            "scheduled project command",
        )?;
        let managed_environment = options
            .managed_environments
            .iter()
            .find(|environment| environment.project_id() == service.project().as_str())
            .cloned()
            .unwrap_or_else(|| empty_environment(service.project().as_str()));
        scheduled_commands.push(
            plan_scheduled_project_command(service, application_service, managed_environment)
                .map_err(invalid)?,
        );
    }

    let gateway = GatewaySnapshot::new(routes).map_err(invalid)?;

    Ok(EngineReconciliationPlan::new(
        applications,
        dedicated_services,
        processes,
        scheduled_commands,
        gateway,
    ))
}

fn resolve_application_dependency<'plan>(
    services: &'plan [crate::control_plane::ServiceExecutionPlan],
    service: &crate::control_plane::ServiceExecutionPlan,
    workload: &str,
) -> Result<&'plan crate::control_plane::ServiceExecutionPlan, EngineReconciliationPlanError> {
    let applications = services
        .iter()
        .filter(|candidate| {
            candidate.project() == service.project()
                && candidate.strategy() == ServiceDeploymentStrategy::ProjectApplication
                && service
                    .desired()
                    .dependencies()
                    .contains(candidate.service())
        })
        .collect::<Vec<_>>();
    match applications.as_slice() {
        [application] => Ok(application),
        _ => Err(invalid(format!(
            "{workload} '{}-{}' must depend on exactly one project application",
            service.project().as_str(),
            service.service().as_str()
        ))),
    }
}

fn resolve_planned_application<'plan>(
    applications: &'plan [crate::control_plane::workload::ImmutableProjectApplicationPlan],
    service: &crate::control_plane::ServiceExecutionPlan,
    application_service: &crate::control_plane::ServiceExecutionPlan,
    workload: &str,
) -> Result<
    &'plan crate::control_plane::workload::ImmutableProjectApplicationPlan,
    EngineReconciliationPlanError,
> {
    applications
        .iter()
        .find(|application| {
            application.request().metadata().project_id() == Some(service.project().as_str())
                && application.request().metadata().resource_id()
                    == Some(application_service.service().as_str())
        })
        .ok_or_else(|| {
            invalid(format!(
                "{workload} '{}-{}' application dependency was not planned",
                service.project().as_str(),
                service.service().as_str()
            ))
        })
}

fn unsupported(
    service: &crate::control_plane::ServiceExecutionPlan,
) -> EngineReconciliationPlanError {
    invalid(format!(
        "service '{}-{}' strategy {:?} has no registered Engine reconciler",
        service.project().as_str(),
        service.service().as_str(),
        service.strategy()
    ))
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
