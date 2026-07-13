use super::{RedisFlavor, RedisPlanError, RedisSharedInstancePlanOptions};
use crate::control_plane::engine::{
    BindMount, ContainerCreateOptions, ContainerRestartPolicy, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ResourceKind, RetentionClass, VolumeCreateOptions, VolumeMount,
};
use crate::control_plane::shared_infrastructure::{
    IsolationCapability, PersistenceMode, SharedInstancePlan,
};
use crate::control_plane::state::{CredentialLifecycle, CredentialRecord, CredentialRecordOptions};

const ACL_MOUNT_TARGET: &str = "/etc/stackctl/acl";
const ACL_FILE: &str = "/etc/stackctl/acl/users.acl";
const DATA_MOUNT_TARGET: &str = "/data";

/// Exact Engine resources for one Redis-compatible compatibility profile.
pub(crate) struct RedisSharedInstancePlan {
    flavor: RedisFlavor,
    container: ContainerCreateOptions,
    volume: Option<VolumeCreateOptions>,
    bootstrap_credential: CredentialRecord,
    command_arguments: Vec<String>,
}

impl RedisSharedInstancePlan {
    pub(crate) fn new(
        shared: &SharedInstancePlan,
        options: RedisSharedInstancePlanOptions,
    ) -> Result<Self, RedisPlanError> {
        let profile = shared.profile();
        let flavor =
            RedisFlavor::from_implementation(profile.implementation()).ok_or_else(|| {
                RedisPlanError::new(format!(
                    "Redis-compatible plan cannot materialize implementation '{}'",
                    profile.implementation()
                ))
            })?;
        if profile.isolation() != IsolationCapability::AclAndPrefix {
            return Err(RedisPlanError::new(
                "Redis-compatible sharing requires acl_and_prefix isolation",
            ));
        }
        if !options.acl_directory.is_absolute() {
            return Err(RedisPlanError::new(format!(
                "Redis ACL directory '{}' must be absolute",
                options.acl_directory.display()
            )));
        }
        let acl_directory = options.acl_directory.to_str().ok_or_else(|| {
            RedisPlanError::new("Redis ACL directory must be valid UTF-8 for the Engine API")
        })?;
        let platform = profile.platform_architecture().ok_or_else(|| {
            RedisPlanError::new("Redis-compatible profile requires a Linux platform")
        })?;
        let fingerprint = profile.fingerprint().as_str();
        let identity = fingerprint
            .strip_prefix("sha256:")
            .ok_or_else(|| RedisPlanError::new("Redis-compatible fingerprint is malformed"))?;
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
            username: "stackctl_admin".to_owned(),
            secret: options.bootstrap_secret.expose().to_owned(),
            lifecycle: CredentialLifecycle::Active,
        });
        let command_arguments = vec![
            flavor.server_executable().to_owned(),
            "--aclfile".to_owned(),
            ACL_FILE.to_owned(),
            "--appendonly".to_owned(),
            if profile.persistence() == PersistenceMode::Persistent {
                "yes".to_owned()
            } else {
                "no".to_owned()
            },
        ];
        let container_metadata = metadata(
            &options,
            ResourceKind::SharedService,
            retention,
            fingerprint,
        )?;
        let acl_mount = BindMount::read_only(acl_directory, ACL_MOUNT_TARGET)
            .map_err(|error| RedisPlanError::new(error.to_string()))?;
        let mut container = ContainerCreateOptions::new(
            &container_name,
            profile.image_digest(),
            container_metadata,
        )
        .and_then(|request| request.with_network(&options.network_name))
        .and_then(|request| request.with_platform(platform))
        .and_then(|request| request.with_command(command_arguments.clone()))
        .map_err(|error| RedisPlanError::new(error.to_string()))?
        .with_bind_mount(acl_mount)
        .with_restart_policy(ContainerRestartPolicy::UnlessStopped);
        let volume = if profile.persistence() == PersistenceMode::Persistent {
            let volume_metadata = metadata(&options, ResourceKind::Volume, retention, fingerprint)?;
            let volume = VolumeCreateOptions::new(&volume_name, volume_metadata)
                .map_err(|error| RedisPlanError::new(error.to_string()))?;
            let mount = VolumeMount::read_write(&volume_name, DATA_MOUNT_TARGET)
                .map_err(|error| RedisPlanError::new(error.to_string()))?;
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
            command_arguments,
        })
    }

    pub(crate) const fn flavor(&self) -> RedisFlavor {
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

    pub(crate) const fn acl_mount_target(&self) -> &'static str {
        ACL_MOUNT_TARGET
    }

    pub(crate) const fn acl_file(&self) -> &'static str {
        ACL_FILE
    }

    pub(crate) const fn data_mount_target(&self) -> &'static str {
        DATA_MOUNT_TARGET
    }

    pub(crate) fn command_arguments(&self) -> &[String] {
        &self.command_arguments
    }
}

fn metadata(
    options: &RedisSharedInstancePlanOptions,
    kind: ResourceKind,
    retention: RetentionClass,
    fingerprint: &str,
) -> Result<ManagedResourceMetadata, RedisPlanError> {
    ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: options.installation_id.clone(),
        kind,
        project_id: None,
        compatibility_fingerprint: fingerprint.to_owned(),
        schema_version: options.schema_version,
        desired_revision: options.desired_revision.clone(),
        retention,
    })
    .map_err(|error| RedisPlanError::new(error.to_string()))
}
