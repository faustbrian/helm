use super::{MySqlFlavor, MySqlPlanError, MySqlSharedInstancePlanOptions};
use crate::control_plane::engine::{
    ContainerCreateOptions, ContainerRestartPolicy, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ResourceKind, RetentionClass, VolumeCreateOptions, VolumeMount,
};
use crate::control_plane::shared_infrastructure::{
    IsolationCapability, PersistenceMode, SharedInstancePlan,
};
use crate::control_plane::state::{CredentialLifecycle, CredentialRecord, CredentialRecordOptions};
use std::collections::BTreeMap;

const DATA_MOUNT_TARGET: &str = "/var/lib/mysql";

/// Exact Engine resources for one MySQL or MariaDB compatibility profile.
pub(crate) struct MySqlSharedInstancePlan {
    flavor: MySqlFlavor,
    container: ContainerCreateOptions,
    volume: Option<VolumeCreateOptions>,
    bootstrap_credential: CredentialRecord,
}

impl MySqlSharedInstancePlan {
    pub(crate) fn new(
        shared: &SharedInstancePlan,
        options: MySqlSharedInstancePlanOptions,
    ) -> Result<Self, MySqlPlanError> {
        let profile = shared.profile();
        let flavor =
            MySqlFlavor::from_implementation(profile.implementation()).ok_or_else(|| {
                MySqlPlanError::new(format!(
                    "MySQL-family plan cannot materialize implementation '{}'",
                    profile.implementation()
                ))
            })?;
        if profile.isolation() != IsolationCapability::DatabaseAndRole {
            return Err(MySqlPlanError::new(
                "MySQL-family sharing requires database_and_role isolation",
            ));
        }
        let platform = profile.platform_architecture().ok_or_else(|| {
            MySqlPlanError::new("MySQL-family compatibility profile requires a Linux platform")
        })?;
        let fingerprint = profile.fingerprint().as_str();
        let identity = fingerprint.strip_prefix("sha256:").ok_or_else(|| {
            MySqlPlanError::new("MySQL-family compatibility fingerprint is malformed")
        })?;
        let container_name = format!("stackctl-shared-{identity}");
        let volume_name = format!("{container_name}-data");
        let retention = match profile.persistence() {
            PersistenceMode::Persistent => RetentionClass::Persistent,
            PersistenceMode::Ephemeral => RetentionClass::Disposable,
        };
        let bootstrap_credential = CredentialRecord::new(CredentialRecordOptions {
            credential_id: format!("shared/{identity}/{}-bootstrap", flavor.implementation()),
            project_id: None,
            service_id: flavor.implementation().to_owned(),
            username: "root".to_owned(),
            secret: options.bootstrap_secret.expose().to_owned(),
            lifecycle: CredentialLifecycle::Active,
        });
        let container_metadata = metadata(
            &options,
            ResourceKind::SharedService,
            retention,
            fingerprint,
        )?;
        let mut container = ContainerCreateOptions::new(
            &container_name,
            profile.image_digest(),
            container_metadata,
        )
        .and_then(|request| request.with_network(&options.network_name))
        .and_then(|request| request.with_platform(platform))
        .and_then(|request| {
            request.with_environment(BTreeMap::from([(
                flavor.root_password_key().to_owned(),
                bootstrap_credential.secret().to_owned(),
            )]))
        })
        .map_err(|error| MySqlPlanError::new(error.to_string()))?
        .with_restart_policy(ContainerRestartPolicy::UnlessStopped);
        let volume = if profile.persistence() == PersistenceMode::Persistent {
            let volume_metadata = metadata(&options, ResourceKind::Volume, retention, fingerprint)?;
            let volume = VolumeCreateOptions::new(&volume_name, volume_metadata)
                .map_err(|error| MySqlPlanError::new(error.to_string()))?;
            let mount = VolumeMount::read_write(&volume_name, DATA_MOUNT_TARGET)
                .map_err(|error| MySqlPlanError::new(error.to_string()))?;
            container = container.with_volume_mount(mount);

            Some(volume)
        } else {
            None
        };

        Ok(Self {
            flavor,
            container,
            volume,
            bootstrap_credential,
        })
    }

    pub(crate) const fn flavor(&self) -> MySqlFlavor {
        self.flavor
    }

    pub(crate) const fn container(&self) -> &ContainerCreateOptions {
        &self.container
    }

    pub(crate) const fn volume(&self) -> Option<&VolumeCreateOptions> {
        self.volume.as_ref()
    }

    pub(crate) const fn bootstrap_credential(&self) -> &CredentialRecord {
        &self.bootstrap_credential
    }

    pub(crate) const fn data_mount_target(&self) -> &'static str {
        DATA_MOUNT_TARGET
    }
}

fn metadata(
    options: &MySqlSharedInstancePlanOptions,
    kind: ResourceKind,
    retention: RetentionClass,
    fingerprint: &str,
) -> Result<ManagedResourceMetadata, MySqlPlanError> {
    ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: options.installation_id.clone(),
        kind,
        project_id: None,
        compatibility_fingerprint: fingerprint.to_owned(),
        schema_version: options.schema_version,
        desired_revision: options.desired_revision.clone(),
        retention,
    })
    .map_err(|error| MySqlPlanError::new(error.to_string()))
}
