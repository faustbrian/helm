use super::{EphemeralBrowserOptions, EphemeralBrowserPlan, WorkloadPlanError};
use crate::control_plane::ServiceDeploymentStrategy;
use crate::control_plane::engine::{
    ContainerCreateOptions, ContainerHealthCheck, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ResourceKind, RetentionClass,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::time::Duration;

const BROWSER_SHARED_MEMORY_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Plans one operation-scoped Selenium sidecar without host exposure.
pub(crate) fn plan_ephemeral_browser(
    options: EphemeralBrowserOptions<'_>,
) -> Result<EphemeralBrowserPlan, WorkloadPlanError> {
    let service = options.service;
    if service.strategy() != ServiceDeploymentStrategy::Ephemeral {
        return Err(invalid(format!(
            "service '{}-{}' is not ephemeral",
            service.project().as_str(),
            service.service().as_str()
        )));
    }
    if options.operation_id.is_empty() {
        return Err(invalid("ephemeral browser operation ID must not be empty"));
    }
    let preset = service.desired().preset().ok_or_else(|| {
        invalid(format!(
            "ephemeral service '{}-{}' requires a browser preset",
            service.project().as_str(),
            service.service().as_str()
        ))
    })?;
    if !matches!(preset, "dusk" | "selenium") {
        return Err(invalid(format!(
            "ephemeral service '{}-{}' preset '{preset}' is not a browser",
            service.project().as_str(),
            service.service().as_str()
        )));
    }
    let image = service.desired().image().ok_or_else(|| {
        invalid(format!(
            "ephemeral browser '{}-{}' requires a resolved immutable image artifact",
            service.project().as_str(),
            service.service().as_str()
        ))
    })?;
    let operation_digest = Sha256::digest(options.operation_id.as_bytes());
    let session = &hex::encode(operation_digest)[..16];
    let container_name = format!("stackctl-ephemeral-{session}");
    let compatibility_fingerprint =
        fingerprint(["ephemeral-browser-v1", preset, image, options.platform]);
    let desired_revision = desired_revision(&options, preset, image)?;
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: options.installation_id.to_owned(),
        kind: ResourceKind::EphemeralService,
        project_id: Some(service.project().as_str().to_owned()),
        compatibility_fingerprint,
        schema_version: options.schema_version,
        desired_revision,
        retention: RetentionClass::Disposable,
    })
    .and_then(|metadata| metadata.with_resource_id(service.service().as_str()))
    .map_err(invalid)?;
    let health_check = ContainerHealthCheck::new(
        vec![
            "/opt/bin/check-grid.sh".to_owned(),
            "--host".to_owned(),
            "0.0.0.0".to_owned(),
            "--port".to_owned(),
            "4444".to_owned(),
        ],
        Duration::from_secs(15),
        Duration::from_secs(30),
        Duration::from_secs(5),
        5,
    )
    .map_err(invalid)?;
    let request = ContainerCreateOptions::new(&container_name, image, metadata)
        .map_err(invalid)?
        .with_platform(options.platform)
        .map_err(invalid)?
        .with_network(options.network_name)
        .map_err(invalid)?
        .with_environment(service.desired().environment().clone())
        .map_err(invalid)?
        .with_shared_memory_bytes(BROWSER_SHARED_MEMORY_BYTES)
        .map_err(invalid)?
        .with_health_check(health_check);
    let command_environment = BTreeMap::from([(
        "DUSK_DRIVER_URL".to_owned(),
        format!("http://{container_name}:4444/wd/hub"),
    )]);

    Ok(EphemeralBrowserPlan::new(request, command_environment))
}

fn desired_revision(
    options: &EphemeralBrowserOptions<'_>,
    preset: &str,
    image: &str,
) -> Result<String, WorkloadPlanError> {
    let manifest = serde_json::to_vec(&EphemeralBrowserRevision {
        schema_version: 1,
        operation_id: options.operation_id,
        project: options.service.project().as_str(),
        service: options.service.service().as_str(),
        preset,
        image,
        platform: options.platform,
        network_name: options.network_name,
        environment: options.service.desired().environment(),
    })
    .map_err(invalid)?;

    Ok(format!("sha256:{}", hex::encode(Sha256::digest(manifest))))
}

#[derive(Serialize)]
struct EphemeralBrowserRevision<'value> {
    schema_version: u32,
    operation_id: &'value str,
    project: &'value str,
    service: &'value str,
    preset: &'value str,
    image: &'value str,
    platform: &'value str,
    network_name: &'value str,
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
