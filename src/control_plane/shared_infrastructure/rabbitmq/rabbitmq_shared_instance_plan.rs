use super::{RabbitMqPlanError, RabbitMqSharedInstancePlanOptions};
use crate::control_plane::engine::{
    BindMount, ContainerCreateOptions, ContainerRestartPolicy, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ResourceKind, RetentionClass, VolumeCreateOptions, VolumeMount,
};
use crate::control_plane::shared_infrastructure::{
    IsolationCapability, PersistenceMode, SharedInstancePlan,
};
use std::collections::BTreeMap;

const CONFIG_MOUNT_TARGET: &str = "/etc/stackctl/rabbitmq";
const CONFIG_FILE: &str = "/etc/stackctl/rabbitmq/rabbitmq.conf";
const DEFINITIONS_FILE: &str = "/etc/stackctl/rabbitmq/definitions.json";
const DATA_MOUNT_TARGET: &str = "/var/lib/rabbitmq";
const NODE_NAME: &str = "rabbit@localhost";

/// Exact Engine resources for one RabbitMQ compatibility profile.
pub(crate) struct RabbitMqSharedInstancePlan {
    container: ContainerCreateOptions,
    volume: Option<VolumeCreateOptions>,
}

impl RabbitMqSharedInstancePlan {
    pub(crate) fn new(
        shared: &SharedInstancePlan,
        options: RabbitMqSharedInstancePlanOptions,
    ) -> Result<Self, RabbitMqPlanError> {
        let profile = shared.profile();
        if profile.implementation() != "rabbitmq" {
            return Err(RabbitMqPlanError::new(format!(
                "RabbitMQ plan cannot materialize implementation '{}'",
                profile.implementation()
            )));
        }
        if profile.isolation() != IsolationCapability::VirtualHostAndUser {
            return Err(RabbitMqPlanError::new(
                "RabbitMQ sharing requires virtual_host_and_user isolation",
            ));
        }
        if !options.definitions_directory.is_absolute() {
            return Err(RabbitMqPlanError::new(format!(
                "RabbitMQ definitions directory '{}' must be absolute",
                options.definitions_directory.display()
            )));
        }
        let definitions_directory = options.definitions_directory.to_str().ok_or_else(|| {
            RabbitMqPlanError::new(
                "RabbitMQ definitions directory must be valid UTF-8 for the Engine API",
            )
        })?;
        let platform = profile.platform_architecture().ok_or_else(|| {
            RabbitMqPlanError::new("RabbitMQ compatibility profile requires a Linux platform")
        })?;
        let fingerprint = profile.fingerprint().as_str();
        let identity = fingerprint
            .strip_prefix("sha256:")
            .ok_or_else(|| RabbitMqPlanError::new("RabbitMQ fingerprint is malformed"))?;
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
        let config_mount = BindMount::read_only(definitions_directory, CONFIG_MOUNT_TARGET)
            .map_err(|error| RabbitMqPlanError::new(error.to_string()))?;
        let mut container = ContainerCreateOptions::new(
            &container_name,
            profile.image_digest(),
            container_metadata,
        )
        .and_then(|request| request.with_network(&options.network_name))
        .and_then(|request| request.with_platform(platform))
        .and_then(|request| {
            request.with_environment(BTreeMap::from([
                ("RABBITMQ_CONFIG_FILE".to_owned(), CONFIG_FILE.to_owned()),
                ("RABBITMQ_NODENAME".to_owned(), NODE_NAME.to_owned()),
            ]))
        })
        .map_err(|error| RabbitMqPlanError::new(error.to_string()))?
        .with_bind_mount(config_mount)
        .with_restart_policy(ContainerRestartPolicy::UnlessStopped);
        let volume = if profile.persistence() == PersistenceMode::Persistent {
            let volume_metadata = metadata(&options, ResourceKind::Volume, retention, fingerprint)?;
            let volume = VolumeCreateOptions::new(&volume_name, volume_metadata)
                .map_err(|error| RabbitMqPlanError::new(error.to_string()))?;
            let mount = VolumeMount::read_write(&volume_name, DATA_MOUNT_TARGET)
                .map_err(|error| RabbitMqPlanError::new(error.to_string()))?;
            container = container.with_volume_mount(mount);

            Some(volume)
        } else {
            None
        };

        Ok(Self { container, volume })
    }

    pub(crate) const fn container(&self) -> &ContainerCreateOptions {
        &self.container
    }

    pub(crate) const fn volume(&self) -> Option<&VolumeCreateOptions> {
        self.volume.as_ref()
    }

    pub(crate) const fn config_mount_target(&self) -> &'static str {
        CONFIG_MOUNT_TARGET
    }

    pub(crate) const fn config_file(&self) -> &'static str {
        CONFIG_FILE
    }

    pub(crate) const fn data_mount_target(&self) -> &'static str {
        DATA_MOUNT_TARGET
    }

    pub(crate) const fn definitions_file(&self) -> &'static str {
        DEFINITIONS_FILE
    }

    pub(crate) const fn node_name(&self) -> &'static str {
        NODE_NAME
    }
}

fn metadata(
    options: &RabbitMqSharedInstancePlanOptions,
    kind: ResourceKind,
    retention: RetentionClass,
    fingerprint: &str,
) -> Result<ManagedResourceMetadata, RabbitMqPlanError> {
    ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: options.installation_id.clone(),
        kind,
        project_id: None,
        compatibility_fingerprint: fingerprint.to_owned(),
        schema_version: options.schema_version,
        desired_revision: options.desired_revision.clone(),
        retention,
    })
    .map_err(|error| RabbitMqPlanError::new(error.to_string()))
}
