use super::{PostgresPlanError, PostgresSharedInstancePlanOptions};
use crate::control_plane::engine::{
    ContainerCreateOptions, ContainerRestartPolicy, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ResourceKind, RetentionClass, VolumeCreateOptions, VolumeMount,
};
use crate::control_plane::shared_infrastructure::{
    IsolationCapability, PersistenceMode, SharedInstancePlan,
};
use std::collections::BTreeMap;

const POSTGRES_LEGACY_DATA_TARGET: &str = "/var/lib/postgresql/data";
const POSTGRES_CURRENT_DATA_TARGET: &str = "/var/lib/postgresql";

/// Exact Engine resources for one compatibility-keyed PostgreSQL process.
pub(crate) struct PostgresSharedInstancePlan {
    container: ContainerCreateOptions,
    volume: Option<VolumeCreateOptions>,
    data_mount_target: String,
}

impl PostgresSharedInstancePlan {
    pub(crate) fn new(
        shared: &SharedInstancePlan,
        options: PostgresSharedInstancePlanOptions,
    ) -> Result<Self, PostgresPlanError> {
        let profile = shared.profile();
        if profile.implementation() != "postgresql" {
            return Err(PostgresPlanError::new(format!(
                "PostgreSQL instance plan cannot materialize implementation '{}'",
                profile.implementation()
            )));
        }
        if profile.isolation() != IsolationCapability::DatabaseAndRole {
            return Err(PostgresPlanError::new(
                "PostgreSQL sharing requires database_and_role isolation",
            ));
        }

        let platform = profile.platform_architecture().ok_or_else(|| {
            PostgresPlanError::new("PostgreSQL compatibility profile requires a Linux platform")
        })?;
        let fingerprint = profile.fingerprint().as_str();
        let identity = fingerprint.strip_prefix("sha256:").ok_or_else(|| {
            PostgresPlanError::new("PostgreSQL compatibility fingerprint is malformed")
        })?;
        let container_name = format!("stackctl-shared-{identity}");
        let volume_name = format!("{container_name}-data");
        let retention = match profile.persistence() {
            PersistenceMode::Persistent => RetentionClass::Persistent,
            PersistenceMode::Ephemeral => RetentionClass::Disposable,
        };
        let container_metadata = metadata(
            &options,
            ResourceKind::SharedService,
            retention,
            fingerprint,
        )?;
        let data_mount_target = data_mount_target(profile.major_version(), profile)?;
        let mut container = ContainerCreateOptions::new(
            &container_name,
            profile.image_digest(),
            container_metadata,
        )
        .and_then(|request| request.with_network(&options.network_name))
        .and_then(|request| request.with_platform(platform))
        .and_then(|request| {
            request.with_environment(BTreeMap::from([
                ("POSTGRES_DB".to_owned(), "postgres".to_owned()),
                ("POSTGRES_USER".to_owned(), "stackctl_admin".to_owned()),
                (
                    "POSTGRES_PASSWORD".to_owned(),
                    options.bootstrap_secret.expose().to_owned(),
                ),
            ]))
        })
        .map_err(|error| PostgresPlanError::new(error.to_string()))?
        .with_restart_policy(ContainerRestartPolicy::UnlessStopped);

        let volume = if profile.persistence() == PersistenceMode::Persistent {
            let volume_metadata = metadata(&options, ResourceKind::Volume, retention, fingerprint)?;
            let volume = VolumeCreateOptions::new(&volume_name, volume_metadata)
                .map_err(|error| PostgresPlanError::new(error.to_string()))?;
            let mount = VolumeMount::read_write(&volume_name, &data_mount_target)
                .map_err(|error| PostgresPlanError::new(error.to_string()))?;
            container = container.with_volume_mount(mount);

            Some(volume)
        } else {
            None
        };

        Ok(Self {
            container,
            volume,
            data_mount_target,
        })
    }

    pub(crate) const fn container(&self) -> &ContainerCreateOptions {
        &self.container
    }

    pub(crate) const fn volume(&self) -> Option<&VolumeCreateOptions> {
        self.volume.as_ref()
    }

    pub(crate) fn data_mount_target(&self) -> &str {
        &self.data_mount_target
    }
}

fn metadata(
    options: &PostgresSharedInstancePlanOptions,
    kind: ResourceKind,
    retention: RetentionClass,
    fingerprint: &str,
) -> Result<ManagedResourceMetadata, PostgresPlanError> {
    ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: options.installation_id.clone(),
        kind,
        project_id: None,
        compatibility_fingerprint: fingerprint.to_owned(),
        schema_version: options.schema_version,
        desired_revision: options.desired_revision.clone(),
        retention,
    })
    .map_err(|error| PostgresPlanError::new(error.to_string()))
}

fn data_mount_target(
    major_version: &str,
    profile: &crate::control_plane::shared_infrastructure::CompatibilityProfile,
) -> Result<String, PostgresPlanError> {
    if let Some(target) = profile.immutable_settings().get("data_mount_target") {
        if !target.starts_with('/') || target.contains('\0') {
            return Err(PostgresPlanError::new(format!(
                "PostgreSQL data mount target '{target}' must be an absolute Linux path"
            )));
        }

        return Ok(target.clone());
    }

    let major = major_version.parse::<u32>().map_err(|_| {
        PostgresPlanError::new(format!(
            "PostgreSQL major version '{major_version}' must be numeric"
        ))
    })?;

    Ok(if major >= 18 {
        POSTGRES_CURRENT_DATA_TARGET.to_owned()
    } else {
        POSTGRES_LEGACY_DATA_TARGET.to_owned()
    })
}
