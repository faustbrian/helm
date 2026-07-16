use super::{
    ApplicationContainerPlan, ApplicationContainerPlanOptions, ApplicationContainerRequestOptions,
    ApplicationHealthCheck, ImmutableProjectApplicationOptions, ImmutableProjectApplicationPlan,
    RuntimeEnvironment, RuntimeEnvironmentOptions, RuntimeImageBuildOptions, RuntimeImageBuildPlan,
    WorkloadPlanError, application_container_request, apply_application_runtime_environment,
    resolve_application_command,
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
    let mut declared_environment = service.desired().environment().clone();
    apply_application_runtime_environment(service, &mut declared_environment);
    let environment = RuntimeEnvironment::new(RuntimeEnvironmentOptions {
        project: service.project().clone(),
        declared: declared_environment,
        managed: options.managed_environment,
    })
    .map_err(invalid)?;
    let runtime_image = if service.desired().php_extensions().is_empty()
        && service.desired().composer_image().is_none()
        && service.desired().node_image().is_none()
        && service.desired().bun_image().is_none()
    {
        None
    } else {
        Some(
            RuntimeImageBuildPlan::for_application_runtime(RuntimeImageBuildOptions {
                installation_id: options.installation_id,
                schema_version: options.schema_version,
                base_image_digest: image,
                platform: options.platform,
                php_extensions: service.desired().php_extensions().to_vec(),
                composer_image: service.desired().composer_image(),
                node_image: service.desired().node_image(),
                bun_image: service.desired().bun_image(),
            })
            .map_err(invalid)?,
        )
    };
    let compatibility_fingerprint = runtime_image.as_ref().map_or_else(
        || compatibility_fingerprint(image, options.platform, options.container_user),
        |runtime| runtime.compatibility_fingerprint().to_owned(),
    );
    let command = resolve_application_command(service, options.internal_http_port);
    let health_check = ApplicationHealthCheck::for_preset(service.desired().preset());
    let desired_revision = desired_revision(ProjectApplicationRevision {
        schema_version: 3,
        project: service.project().as_str(),
        service: service.service().as_str(),
        image,
        platform: options.platform,
        container_user: options.container_user,
        network_name: options.network_name,
        internal_http_port: options.internal_http_port,
        command: &command,
        health_check,
        managed_environment_revision: environment.managed_revision(),
        environment: environment.values(),
        php_extensions: service.desired().php_extensions(),
        composer_image: service.desired().composer_image(),
        node_image: service.desired().node_image(),
        bun_image: service.desired().bun_image(),
    })?;
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
        health_check,
    })
    .map_err(invalid)?;
    let route = container.gateway_route().clone();
    let request = application_container_request(ApplicationContainerRequestOptions {
        plan: container,
        metadata,
        platform: options.platform.to_owned(),
        container_user: options.container_user.to_owned(),
        command,
        environment,
    })
    .map_err(invalid)?;

    Ok(ImmutableProjectApplicationPlan::new(
        request,
        route,
        runtime_image,
    ))
}

fn compatibility_fingerprint(image: &str, platform: &str, container_user: &str) -> String {
    fingerprint([
        "project-application-runtime-v2",
        image,
        platform,
        container_user,
    ])
}

fn desired_revision(manifest: ProjectApplicationRevision<'_>) -> Result<String, WorkloadPlanError> {
    let manifest = serde_json::to_vec(&manifest).map_err(invalid)?;

    Ok(format!("sha256:{}", hex::encode(Sha256::digest(manifest))))
}

#[derive(Serialize)]
struct ProjectApplicationRevision<'value> {
    schema_version: u32,
    project: &'value str,
    service: &'value str,
    image: &'value str,
    platform: &'value str,
    container_user: &'value str,
    network_name: &'value str,
    internal_http_port: u16,
    command: &'value [String],
    health_check: ApplicationHealthCheck,
    managed_environment_revision: &'value str,
    environment: &'value BTreeMap<String, String>,
    php_extensions: &'value [String],
    composer_image: Option<&'value str>,
    node_image: Option<&'value str>,
    bun_image: Option<&'value str>,
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
