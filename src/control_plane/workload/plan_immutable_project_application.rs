use super::{
    ApplicationContainerPlan, ApplicationContainerPlanOptions, ApplicationContainerRequestOptions,
    ImmutableProjectApplicationOptions, ImmutableProjectApplicationPlan, RuntimeEnvironment,
    RuntimeEnvironmentOptions, WorkloadPlanError, application_container_request,
};
use crate::control_plane::ServiceDeploymentStrategy;
use crate::control_plane::engine::{
    ManagedResourceMetadata, ManagedResourceMetadataOptions, ResourceKind, RetentionClass,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Resolves one immutable custom image into an exact owned Engine operation.
pub(crate) fn plan_immutable_project_application(
    options: ImmutableProjectApplicationOptions<'_>,
) -> Result<ImmutableProjectApplicationPlan, WorkloadPlanError> {
    let service = options.service;
    if service.strategy() != ServiceDeploymentStrategy::ProjectApplication {
        return Err(invalid(format!(
            "service '{}-{}' is not a project application",
            service.project().as_str(),
            service.service().as_str()
        )));
    }
    let image = service.desired().image().ok_or_else(|| {
        invalid(format!(
            "project application '{}-{}' requires a resolved immutable image artifact",
            service.project().as_str(),
            service.service().as_str()
        ))
    })?;
    let environment = RuntimeEnvironment::new(RuntimeEnvironmentOptions {
        project: service.project().clone(),
        declared: service.desired().environment().clone(),
        managed: options.managed_environment,
    })
    .map_err(invalid)?;
    let compatibility_fingerprint = compatibility_fingerprint(image, options.platform);
    let desired_revision = desired_revision(
        service,
        image,
        options.platform,
        options.network_name,
        options.internal_http_port,
        &environment,
    )?;
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: options.installation_id.to_owned(),
        kind: ResourceKind::ProjectApplication,
        project_id: Some(service.project().as_str().to_owned()),
        compatibility_fingerprint,
        schema_version: options.schema_version,
        desired_revision,
        retention: RetentionClass::Disposable,
    })
    .and_then(|metadata| metadata.with_resource_id(service.service().as_str()))
    .map_err(invalid)?;
    let container = ApplicationContainerPlan::new(ApplicationContainerPlanOptions {
        project: service.project().clone(),
        service: service.service().clone(),
        image_digest: image.to_owned(),
        source_path: service.project_directory().to_path_buf(),
        network_name: options.network_name.to_owned(),
        internal_http_port: options.internal_http_port,
    })
    .map_err(invalid)?;
    let route = container.gateway_route().clone();
    let request = application_container_request(ApplicationContainerRequestOptions {
        plan: container,
        metadata,
        platform: options.platform.to_owned(),
        command: service.desired().command().unwrap_or_default().to_vec(),
        environment,
    })
    .map_err(invalid)?;

    Ok(ImmutableProjectApplicationPlan::new(request, route))
}

fn compatibility_fingerprint(image: &str, platform: &str) -> String {
    fingerprint(["project-application-runtime-v1", image, platform])
}

fn desired_revision(
    service: &crate::control_plane::ServiceExecutionPlan,
    image: &str,
    platform: &str,
    network_name: &str,
    internal_http_port: u16,
    environment: &RuntimeEnvironment,
) -> Result<String, WorkloadPlanError> {
    let manifest = serde_json::to_vec(&ProjectApplicationRevision {
        schema_version: 1,
        project: service.project().as_str(),
        service: service.service().as_str(),
        image,
        platform,
        network_name,
        internal_http_port,
        command: service.desired().command().unwrap_or_default(),
        managed_environment_revision: environment.managed_revision(),
        environment: environment.values(),
    })
    .map_err(invalid)?;

    Ok(format!("sha256:{}", hex::encode(Sha256::digest(manifest))))
}

#[derive(Serialize)]
struct ProjectApplicationRevision<'value> {
    schema_version: u32,
    project: &'value str,
    service: &'value str,
    image: &'value str,
    platform: &'value str,
    network_name: &'value str,
    internal_http_port: u16,
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
