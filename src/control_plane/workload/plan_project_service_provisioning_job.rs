use super::WorkloadPlanError;
use crate::control_plane::ServiceExecutionPlan;
use crate::control_plane::engine::{
    ContainerCreateOptions, ManagedResourceMetadata, ManagedResourceMetadataOptions, ResourceKind,
    RetentionClass,
};
use crate::control_plane::project_infrastructure::ProjectServiceProvisioningJob;
use sha2::{Digest, Sha256};

/// Converts one adapter provisioning intent into an owned disposable job.
pub(super) fn plan_project_service_provisioning_job(
    service: &ServiceExecutionPlan,
    provisioning: &ProjectServiceProvisioningJob,
    installation_id: &str,
    schema_version: u32,
    platform: &str,
    network_name: &str,
) -> Result<ContainerCreateOptions, WorkloadPlanError> {
    let revision = serde_json::to_vec(&(
        provisioning.image(),
        provisioning.command(),
        provisioning.environment(),
        platform,
        network_name,
    ))
    .map_err(invalid)?;
    let fingerprint = format!(
        "sha256:{}",
        hex::encode(Sha256::digest(provisioning.image()))
    );
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.to_owned(),
        kind: ResourceKind::ProvisioningJob,
        project_id: Some(service.project().as_str().to_owned()),
        compatibility_fingerprint: fingerprint,
        schema_version,
        desired_revision: format!("sha256:{}", hex::encode(Sha256::digest(revision))),
        retention: RetentionClass::Disposable,
    })
    .and_then(|metadata| metadata.with_resource_id(service.service().as_str()))
    .map_err(invalid)?;

    ContainerCreateOptions::new(
        format!(
            "stackctl-job-{}-{}-provision",
            service.project().as_str(),
            service.service().as_str()
        ),
        provisioning.image(),
        metadata,
    )
    .and_then(|request| request.with_platform(platform))
    .and_then(|request| request.with_network(network_name))
    .and_then(|request| request.with_command(provisioning.command().to_vec()))
    .and_then(|request| request.with_environment(provisioning.environment().clone()))
    .map_err(invalid)
}

fn invalid(error: impl std::fmt::Display) -> WorkloadPlanError {
    WorkloadPlanError::new(error.to_string())
}
