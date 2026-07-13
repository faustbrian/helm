use super::{GotenbergPlanError, GotenbergSharedInstancePlanOptions};
use crate::control_plane::engine::{
    ContainerCreateOptions, ContainerHealthCheck, ContainerRestartPolicy, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ResourceKind, RetentionClass,
};
use crate::control_plane::shared_infrastructure::{
    IsolationCapability, PersistenceMode, SharedInstancePlan,
};
use std::time::Duration;

/// Exact Engine request for one immutable stateless Gotenberg profile.
pub(crate) struct GotenbergSharedInstancePlan {
    container: ContainerCreateOptions,
}

impl GotenbergSharedInstancePlan {
    pub(crate) fn new(
        shared: &SharedInstancePlan,
        options: GotenbergSharedInstancePlanOptions,
    ) -> Result<Self, GotenbergPlanError> {
        let profile = shared.profile();
        if profile.implementation() != "gotenberg" {
            return Err(GotenbergPlanError::new(format!(
                "Gotenberg plan cannot materialize implementation '{}'",
                profile.implementation()
            )));
        }
        if profile.persistence() != PersistenceMode::Ephemeral
            || profile.isolation() != IsolationCapability::None
        {
            return Err(GotenbergPlanError::new(
                "Gotenberg sharing requires an ephemeral profile without logical isolation",
            ));
        }
        if !profile.extensions().is_empty() || !profile.immutable_settings().is_empty() {
            return Err(GotenbergPlanError::new(
                "Gotenberg extensions or settings require a materialized configuration adapter",
            ));
        }
        let platform = profile.platform_architecture().ok_or_else(|| {
            GotenbergPlanError::new("Gotenberg compatibility profile requires a Linux platform")
        })?;
        let fingerprint = profile.fingerprint().as_str();
        let identity = fingerprint.strip_prefix("sha256:").ok_or_else(|| {
            GotenbergPlanError::new("Gotenberg compatibility fingerprint is malformed")
        })?;
        let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
            installation_id: options.installation_id,
            kind: ResourceKind::SharedService,
            project_id: None,
            compatibility_fingerprint: fingerprint.to_owned(),
            schema_version: options.schema_version,
            desired_revision: options.desired_revision,
            retention: RetentionClass::Disposable,
        })
        .map_err(|error| GotenbergPlanError::new(error.to_string()))?;
        let health_check = ContainerHealthCheck::new(
            vec![
                "curl".to_owned(),
                "--fail".to_owned(),
                "--silent".to_owned(),
                "http://127.0.0.1:3000/health".to_owned(),
            ],
            Duration::from_secs(5),
            Duration::from_secs(3),
            Duration::from_secs(20),
            12,
        )
        .map_err(|error| GotenbergPlanError::new(error.to_string()))?;
        let container = ContainerCreateOptions::new(
            format!("stackctl-shared-{identity}"),
            profile.image_digest(),
            metadata,
        )
        .and_then(|request| request.with_network(options.network_name))
        .and_then(|request| request.with_platform(platform))
        .map_err(|error| GotenbergPlanError::new(error.to_string()))?
        .with_health_check(health_check)
        .with_restart_policy(ContainerRestartPolicy::UnlessStopped);

        Ok(Self { container })
    }

    pub(crate) const fn container(&self) -> &ContainerCreateOptions {
        &self.container
    }
}
