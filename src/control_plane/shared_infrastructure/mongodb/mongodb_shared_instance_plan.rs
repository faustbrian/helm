use super::{MongoDbPlanError, MongoDbSharedInstancePlanOptions};
use crate::control_plane::engine::{
    BindMount, ContainerCreateOptions, ContainerRestartPolicy, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ResourceKind, RetentionClass, VolumeCreateOptions, VolumeMount,
};
use crate::control_plane::shared_infrastructure::{
    IsolationCapability, PersistenceMode, SharedInstancePlan,
};
use crate::control_plane::state::{CredentialLifecycle, CredentialRecord, CredentialRecordOptions};
use std::collections::BTreeMap;

const DATA_MOUNT_TARGET: &str = "/data/db";
const BOOTSTRAP_SECRET_TARGET: &str = "/run/stackctl-secrets/mongodb-root-password";

/// Exact Engine resources for one MongoDB compatibility profile.
pub(crate) struct MongoDbSharedInstancePlan {
    container: ContainerCreateOptions,
    volume: Option<VolumeCreateOptions>,
    bootstrap_credential: CredentialRecord,
}

impl MongoDbSharedInstancePlan {
    pub(crate) fn new(
        shared: &SharedInstancePlan,
        options: MongoDbSharedInstancePlanOptions,
    ) -> Result<Self, MongoDbPlanError> {
        let profile = shared.profile();
        if profile.implementation() != "mongodb" {
            return Err(MongoDbPlanError::new(format!(
                "MongoDB plan cannot materialize implementation '{}'",
                profile.implementation()
            )));
        }
        if profile.isolation() != IsolationCapability::DatabaseAndRole {
            return Err(MongoDbPlanError::new(
                "MongoDB sharing requires database_and_role isolation",
            ));
        }
        if !options.bootstrap_secret_file.is_absolute() {
            return Err(MongoDbPlanError::new(format!(
                "MongoDB bootstrap secret file '{}' must be absolute",
                options.bootstrap_secret_file.display()
            )));
        }
        let secret_file = options.bootstrap_secret_file.to_str().ok_or_else(|| {
            MongoDbPlanError::new(
                "MongoDB bootstrap secret path must be valid UTF-8 for the Engine API",
            )
        })?;
        let platform = profile.platform_architecture().ok_or_else(|| {
            MongoDbPlanError::new("MongoDB compatibility profile requires a Linux platform")
        })?;
        let fingerprint = profile.fingerprint().as_str();
        let identity = fingerprint
            .strip_prefix("sha256:")
            .ok_or_else(|| MongoDbPlanError::new("MongoDB fingerprint is malformed"))?;
        let container_name = format!("stackctl-shared-{identity}");
        let volume_name = format!("{container_name}-data");
        let retention = match profile.persistence() {
            PersistenceMode::Persistent => RetentionClass::Persistent,
            PersistenceMode::Ephemeral => RetentionClass::Disposable,
        };
        let bootstrap_credential = CredentialRecord::new(CredentialRecordOptions {
            credential_id: format!("shared/{identity}/mongodb-bootstrap"),
            project_id: None,
            service_id: "mongodb".to_owned(),
            username: "stackctl_admin".to_owned(),
            secret: options.bootstrap_secret.expose().to_owned(),
            lifecycle: CredentialLifecycle::Active,
        });
        let secret_mount = BindMount::read_only(secret_file, BOOTSTRAP_SECRET_TARGET)
            .map_err(|error| MongoDbPlanError::new(error.to_string()))?;
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
            request.with_environment(BTreeMap::from([
                (
                    "MONGO_INITDB_ROOT_USERNAME".to_owned(),
                    bootstrap_credential.username().to_owned(),
                ),
                (
                    "MONGO_INITDB_ROOT_PASSWORD_FILE".to_owned(),
                    BOOTSTRAP_SECRET_TARGET.to_owned(),
                ),
            ]))
        })
        .map_err(|error| MongoDbPlanError::new(error.to_string()))?
        .with_bind_mount(secret_mount)
        .with_restart_policy(ContainerRestartPolicy::UnlessStopped);
        let volume = if profile.persistence() == PersistenceMode::Persistent {
            let volume_metadata = metadata(&options, ResourceKind::Volume, retention, fingerprint)?;
            let volume = VolumeCreateOptions::new(&volume_name, volume_metadata)
                .map_err(|error| MongoDbPlanError::new(error.to_string()))?;
            let mount = VolumeMount::read_write(&volume_name, DATA_MOUNT_TARGET)
                .map_err(|error| MongoDbPlanError::new(error.to_string()))?;
            container = container.with_volume_mount(mount);

            Some(volume)
        } else {
            None
        };

        Ok(Self {
            container,
            volume,
            bootstrap_credential,
        })
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

    pub(crate) const fn bootstrap_secret_target(&self) -> &'static str {
        BOOTSTRAP_SECRET_TARGET
    }
}

fn metadata(
    options: &MongoDbSharedInstancePlanOptions,
    kind: ResourceKind,
    retention: RetentionClass,
    fingerprint: &str,
) -> Result<ManagedResourceMetadata, MongoDbPlanError> {
    ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: options.installation_id.clone(),
        kind,
        project_id: None,
        compatibility_fingerprint: fingerprint.to_owned(),
        schema_version: options.schema_version,
        desired_revision: options.desired_revision.clone(),
        retention,
    })
    .map_err(|error| MongoDbPlanError::new(error.to_string()))
}
