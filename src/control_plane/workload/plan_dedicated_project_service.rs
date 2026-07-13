use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{DedicatedProjectServiceOptions, DedicatedProjectServicePlan, WorkloadPlanError};
use crate::control_plane::ServiceDeploymentStrategy;
use crate::control_plane::engine::{
    ContainerCreateOptions, ContainerRestartPolicy, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ResourceKind, RetentionClass, VolumeCreateOptions, VolumeMount,
};

/// Plans one isolated project service without exposing a host port or route.
pub(crate) fn plan_dedicated_project_service(
    options: DedicatedProjectServiceOptions<'_>,
) -> Result<DedicatedProjectServicePlan, WorkloadPlanError> {
    let service = options.service;
    if !matches!(
        service.strategy(),
        ServiceDeploymentStrategy::DedicatedProject
            | ServiceDeploymentStrategy::DedicatedUntilIsolationProven
    ) {
        return Err(invalid(format!(
            "service '{}-{}' is not a dedicated project service",
            service.project().as_str(),
            service.service().as_str()
        )));
    }
    let image = service.desired().image().ok_or_else(|| {
        invalid(format!(
            "dedicated service '{}-{}' requires a resolved immutable image artifact",
            service.project().as_str(),
            service.service().as_str()
        ))
    })?;
    let preset = service.desired().preset().ok_or_else(|| {
        invalid(format!(
            "dedicated service '{}-{}' requires an explicit implementation preset",
            service.project().as_str(),
            service.service().as_str()
        ))
    })?;
    let version = service.desired().version().ok_or_else(|| {
        invalid(format!(
            "dedicated service '{}-{}' requires an exact major version",
            service.project().as_str(),
            service.service().as_str()
        ))
    })?;
    if version.is_empty() || !version.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid(format!(
            "dedicated service '{}-{}' major version '{version}' must be numeric",
            service.project().as_str(),
            service.service().as_str()
        )));
    }

    let compatibility_fingerprint = fingerprint([
        "project-service-v1",
        preset,
        version,
        image,
        options.platform,
    ]);
    let desired_revision = desired_revision(&options, preset, version, image)?;
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: options.installation_id.to_owned(),
        kind: ResourceKind::ProjectService,
        project_id: Some(service.project().as_str().to_owned()),
        compatibility_fingerprint: compatibility_fingerprint.clone(),
        schema_version: options.schema_version,
        desired_revision,
        retention: RetentionClass::Disposable,
    })
    .and_then(|metadata| metadata.with_resource_id(service.service().as_str()))
    .map_err(invalid)?;
    let volume = retained_volume(&options, preset, &compatibility_fingerprint)?;
    let request = ContainerCreateOptions::new(
        format!(
            "stackctl-{}-{}",
            service.project().as_str(),
            service.service().as_str()
        ),
        image,
        metadata,
    )
    .map_err(invalid)?
    .with_platform(options.platform)
    .map_err(invalid)?
    .with_network(options.network_name)
    .map_err(invalid)?
    .with_environment(service.desired().environment().clone())
    .map_err(invalid)?;
    let request = match (&volume, persistent_data_path(preset)) {
        (Some(volume), Some(target)) => request
            .with_volume_mount(VolumeMount::read_write(volume.name(), target).map_err(invalid)?),
        (None, None) => request,
        _ => unreachable!("retained volume and preset mount contract move together"),
    };
    let request = match service.desired().command() {
        Some(command) => request.with_command(command.to_vec()).map_err(invalid)?,
        None => request,
    };

    Ok(DedicatedProjectServicePlan::new(
        request.with_restart_policy(ContainerRestartPolicy::UnlessStopped),
        volume,
    ))
}

fn retained_volume(
    options: &DedicatedProjectServiceOptions<'_>,
    preset: &str,
    compatibility_fingerprint: &str,
) -> Result<Option<VolumeCreateOptions>, WorkloadPlanError> {
    if persistent_data_path(preset).is_none() {
        return Ok(None);
    }
    let service = options.service;
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: options.installation_id.to_owned(),
        kind: ResourceKind::Volume,
        project_id: Some(service.project().as_str().to_owned()),
        compatibility_fingerprint: compatibility_fingerprint.to_owned(),
        schema_version: options.schema_version,
        desired_revision: fingerprint(["project-service-volume-v1", compatibility_fingerprint]),
        retention: RetentionClass::Persistent,
    })
    .and_then(|metadata| metadata.with_resource_id(service.service().as_str()))
    .map_err(invalid)?;

    VolumeCreateOptions::new(
        format!(
            "stackctl-{}-{}-data",
            service.project().as_str(),
            service.service().as_str()
        ),
        metadata,
    )
    .map(Some)
    .map_err(invalid)
}

fn persistent_data_path(preset: &str) -> Option<&'static str> {
    match preset {
        "dragonfly" | "rustfs" | "typesense" => Some("/data"),
        "garage" => Some("/var/lib/garage"),
        "localstack" => Some("/var/lib/localstack"),
        "opensearch" => Some("/usr/share/opensearch/data"),
        "elasticsearch" => Some("/usr/share/elasticsearch/data"),
        "meilisearch" => Some("/meili_data"),
        "memcached" | "mailhog" | "soketi" => None,
        _ => None,
    }
}

fn desired_revision(
    options: &DedicatedProjectServiceOptions<'_>,
    preset: &str,
    version: &str,
    image: &str,
) -> Result<String, WorkloadPlanError> {
    let service = options.service;
    let manifest = serde_json::to_vec(&DedicatedProjectServiceRevision {
        schema_version: 1,
        project: service.project().as_str(),
        service: service.service().as_str(),
        preset,
        version,
        image,
        platform: options.platform,
        network_name: options.network_name,
        command: service.desired().command().unwrap_or_default(),
        environment: service.desired().environment(),
    })
    .map_err(invalid)?;

    Ok(format!("sha256:{}", hex::encode(Sha256::digest(manifest))))
}

#[derive(Serialize)]
struct DedicatedProjectServiceRevision<'value> {
    schema_version: u32,
    project: &'value str,
    service: &'value str,
    preset: &'value str,
    version: &'value str,
    image: &'value str,
    platform: &'value str,
    network_name: &'value str,
    command: &'value [String],
    environment: &'value std::collections::BTreeMap<String, String>,
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
