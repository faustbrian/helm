use super::{
    ProjectProcessOperationOptions, ProjectProcessPlan, ProjectProcessPlanOptions,
    ProjectProcessRequestOptions, RuntimeEnvironment, RuntimeEnvironmentOptions, WorkloadPlanError,
    project_process_request,
};
use crate::control_plane::ServiceDeploymentStrategy;
use crate::control_plane::engine::{
    ContainerCreateOptions, ManagedResourceMetadata, ManagedResourceMetadataOptions, ResourceKind,
    RetentionClass,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Plans one worker or scheduler against its application's exact runtime image.
pub(crate) fn plan_project_process_operation(
    options: ProjectProcessOperationOptions<'_>,
) -> Result<ContainerCreateOptions, WorkloadPlanError> {
    let service = options.service;
    let application_service = options.application_service;
    if service.strategy() != ServiceDeploymentStrategy::ProjectProcess {
        return Err(invalid(format!(
            "service '{}-{}' is not a project process",
            service.project().as_str(),
            service.service().as_str()
        )));
    }
    if application_service.strategy() != ServiceDeploymentStrategy::ProjectApplication
        || service.project() != application_service.project()
        || options.application.request().metadata().project_id() != Some(service.project().as_str())
        || options.application.request().metadata().resource_id()
            != Some(application_service.service().as_str())
    {
        return Err(invalid(format!(
            "project process '{}-{}' requires a matching project application runtime",
            service.project().as_str(),
            service.service().as_str()
        )));
    }
    if service
        .desired()
        .image()
        .is_some_and(|image| image != options.application.request().image())
    {
        return Err(invalid(format!(
            "project process '{}-{}' cannot override application image '{}'",
            service.project().as_str(),
            service.service().as_str(),
            options.application.request().image()
        )));
    }
    let declared = merged_declared_environment(service, application_service)?;
    let environment = RuntimeEnvironment::new(RuntimeEnvironmentOptions {
        project: service.project().clone(),
        declared,
        managed: options.managed_environment,
    })
    .map_err(invalid)?;
    let command = service
        .desired()
        .command()
        .map(<[String]>::to_vec)
        .unwrap_or_else(|| default_command(service.desired().preset().unwrap_or_default()));
    let plan = ProjectProcessPlan::new(ProjectProcessPlanOptions {
        project: service.project().clone(),
        service: service.service().clone(),
        image_digest: options.application.request().image().to_owned(),
        source_path: service.project_directory().to_path_buf(),
        network_name: options.network_name.to_owned(),
        command,
        environment,
    })
    .map_err(invalid)?;
    let compatibility_fingerprint = fingerprint([
        "project-process-runtime-v1",
        plan.image_digest(),
        options.platform,
    ]);
    let desired_revision = desired_revision(&plan, options.platform)?;
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: options.installation_id.to_owned(),
        kind: ResourceKind::ProjectProcess,
        project_id: Some(service.project().as_str().to_owned()),
        compatibility_fingerprint,
        schema_version: options.schema_version,
        desired_revision,
        retention: RetentionClass::Disposable,
    })
    .and_then(|metadata| metadata.with_resource_id(service.service().as_str()))
    .map_err(invalid)?;

    project_process_request(ProjectProcessRequestOptions {
        plan,
        metadata,
        platform: options.platform.to_owned(),
    })
    .map_err(invalid)
}

fn merged_declared_environment(
    service: &crate::control_plane::ServiceExecutionPlan,
    application: &crate::control_plane::ServiceExecutionPlan,
) -> Result<BTreeMap<String, String>, WorkloadPlanError> {
    let mut declared = application.desired().environment().clone();
    for (key, value) in service.desired().environment() {
        if declared.get(key).is_some_and(|existing| existing != value) {
            return Err(invalid(format!(
                "project process '{}-{}' environment key '{key}' conflicts with application '{}'",
                service.project().as_str(),
                service.service().as_str(),
                application.service().as_str()
            )));
        }
        declared.insert(key.clone(), value.clone());
    }

    Ok(declared)
}

fn default_command(preset: &str) -> Vec<String> {
    let arguments = match preset {
        "horizon" => ["php", "artisan", "horizon"].as_slice(),
        "queue-worker" | "queue" => ["php", "artisan", "queue:work", "--no-interaction"].as_slice(),
        "scheduler" => ["php", "artisan", "schedule:work", "--no-interaction"].as_slice(),
        _ => [].as_slice(),
    };

    arguments
        .iter()
        .map(|argument| (*argument).to_owned())
        .collect()
}

fn desired_revision(
    plan: &ProjectProcessPlan,
    platform: &str,
) -> Result<String, WorkloadPlanError> {
    let manifest = serde_json::to_vec(&ProjectProcessRevision {
        schema_version: 1,
        project: plan.project_id(),
        service: plan.service_id(),
        image: plan.image_digest(),
        platform,
        network: plan.network_name(),
        command: plan.command(),
        managed_environment_revision: plan.environment().managed_revision(),
        environment: plan.environment().values(),
    })
    .map_err(invalid)?;

    Ok(format!("sha256:{}", hex::encode(Sha256::digest(manifest))))
}

#[derive(Serialize)]
struct ProjectProcessRevision<'value> {
    schema_version: u32,
    project: &'value str,
    service: &'value str,
    image: &'value str,
    platform: &'value str,
    network: &'value str,
    command: &'value [String],
    managed_environment_revision: &'value str,
    environment: &'value BTreeMap<String, String>,
}

fn fingerprint<'value>(values: impl IntoIterator<Item = &'value str>) -> String {
    let mut hasher = Sha256::new();
    for value in values {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }

    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn invalid(error: impl std::fmt::Display) -> WorkloadPlanError {
    WorkloadPlanError::new(error.to_string())
}
