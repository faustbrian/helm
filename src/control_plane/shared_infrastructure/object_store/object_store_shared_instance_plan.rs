use super::{ObjectStoreFlavor, ObjectStorePlanError, ObjectStoreSharedInstancePlanOptions};
use crate::control_plane::engine::{
    BindMount, ContainerCreateOptions, ContainerHealthCheck, ContainerRestartPolicy,
    ManagedResourceMetadata, ManagedResourceMetadataOptions, ResourceKind, RetentionClass,
    VolumeCreateOptions, VolumeMount,
};
use crate::control_plane::shared_infrastructure::{
    IsolationCapability, PersistenceMode, SharedInstancePlan,
};
use crate::control_plane::state::{CredentialLifecycle, CredentialRecord, CredentialRecordOptions};
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};
use std::time::Duration;

const DATA_MOUNT_TARGET: &str = "/data";
const POLICY_MOUNT_TARGET: &str = "/etc/stackctl/object-store/policies";
const ROOT_USERNAME: &str = "stackctl_admin";

/// Exact Engine resources for one object-store compatibility profile.
pub(crate) struct ObjectStoreSharedInstancePlan {
    flavor: ObjectStoreFlavor,
    container: ContainerCreateOptions,
    volume: Option<VolumeCreateOptions>,
    root_credential: CredentialRecord,
}

impl ObjectStoreSharedInstancePlan {
    pub(crate) fn new(
        shared: &SharedInstancePlan,
        options: ObjectStoreSharedInstancePlanOptions,
    ) -> Result<Self, ObjectStorePlanError> {
        let profile = shared.profile();
        let flavor =
            ObjectStoreFlavor::from_implementation(profile.implementation()).ok_or_else(|| {
                ObjectStorePlanError::new(format!(
                    "object-store plan cannot materialize implementation '{}'",
                    profile.implementation()
                ))
            })?;
        if profile.isolation() != IsolationCapability::BucketAndPolicy {
            return Err(ObjectStorePlanError::new(
                "object-store sharing requires bucket_and_policy isolation",
            ));
        }
        if !options.policy_directory.is_absolute() {
            return Err(ObjectStorePlanError::new(format!(
                "object-store policy directory '{}' must be absolute",
                options.policy_directory.display()
            )));
        }
        let policy_directory = options.policy_directory.to_str().ok_or_else(|| {
            ObjectStorePlanError::new(
                "object-store policy directory must be valid UTF-8 for the Engine API",
            )
        })?;
        if options.root_secret.expose().is_empty() || options.root_secret.expose().contains('\0') {
            return Err(ObjectStorePlanError::new(
                "object-store root secret must be non-empty and contain no NUL bytes",
            ));
        }
        let platform = profile.platform_architecture().ok_or_else(|| {
            ObjectStorePlanError::new(
                "object-store compatibility profile requires a Linux platform",
            )
        })?;
        let fingerprint = profile.fingerprint().as_str();
        let identity = fingerprint.strip_prefix("sha256:").ok_or_else(|| {
            ObjectStorePlanError::new("object-store compatibility fingerprint is malformed")
        })?;
        let container_name = format!("stackctl-shared-{identity}");
        let volume_name = format!("{container_name}-data");
        let retention = match profile.persistence() {
            PersistenceMode::Persistent => RetentionClass::Persistent,
            PersistenceMode::Ephemeral => RetentionClass::Disposable,
        };
        let root_credential = CredentialRecord::new(CredentialRecordOptions {
            credential_id: format!("shared/{identity}/{}-root", flavor.implementation()),
            project_id: None,
            service_id: flavor.implementation().to_owned(),
            username: ROOT_USERNAME.to_owned(),
            secret: options.root_secret.expose().to_owned(),
            lifecycle: CredentialLifecycle::Active,
        });
        let (root_user_key, root_secret_key) = flavor.root_environment();
        let environment = BTreeMap::from([
            (root_user_key.to_owned(), ROOT_USERNAME.to_owned()),
            (
                root_secret_key.to_owned(),
                options.root_secret.expose().to_owned(),
            ),
        ]);
        let health_check = ContainerHealthCheck::new(
            vec![
                "curl".to_owned(),
                "--fail".to_owned(),
                "--silent".to_owned(),
                format!("http://127.0.0.1:9000{}", flavor.readiness_path()),
            ],
            Duration::from_secs(5),
            Duration::from_secs(2),
            Duration::from_secs(10),
            12,
        )
        .map_err(|error| ObjectStorePlanError::new(error.to_string()))?;
        let container_metadata = metadata(
            &options,
            ResourceKind::SharedService,
            retention,
            fingerprint,
        )?
        .with_compatibility_profile(profile.implementation(), profile.major_version())
        .map_err(|error| ObjectStorePlanError::new(error.to_string()))?;
        let policy_mount = BindMount::read_only(policy_directory, POLICY_MOUNT_TARGET)
            .map_err(|error| ObjectStorePlanError::new(error.to_string()))?;
        let mut container = ContainerCreateOptions::new(
            &container_name,
            profile.image_digest(),
            container_metadata,
        )
        .and_then(|request| request.with_network(&options.network_name))
        .and_then(|request| request.with_platform(platform))
        .and_then(|request| request.with_environment(environment))
        .map_err(|error| ObjectStorePlanError::new(error.to_string()))?;
        if flavor == ObjectStoreFlavor::Minio {
            container = container
                .with_command(vec!["server".to_owned(), DATA_MOUNT_TARGET.to_owned()])
                .map_err(|error| ObjectStorePlanError::new(error.to_string()))?;
        }
        container = container
            .with_bind_mount(policy_mount)
            .with_health_check(health_check)
            .with_restart_policy(ContainerRestartPolicy::UnlessStopped);
        let volume = if profile.persistence() == PersistenceMode::Persistent {
            let volume_metadata = metadata(&options, ResourceKind::Volume, retention, fingerprint)?;
            let volume = VolumeCreateOptions::new(&volume_name, volume_metadata)
                .map_err(|error| ObjectStorePlanError::new(error.to_string()))?;
            let mount = VolumeMount::read_write(&volume_name, DATA_MOUNT_TARGET)
                .map_err(|error| ObjectStorePlanError::new(error.to_string()))?;
            container = container.with_volume_mount(mount);

            Some(volume)
        } else {
            None
        };

        Ok(Self {
            flavor,
            container,
            volume,
            root_credential,
        })
    }

    pub(crate) const fn flavor(&self) -> ObjectStoreFlavor {
        self.flavor
    }

    pub(crate) const fn container(&self) -> &ContainerCreateOptions {
        &self.container
    }

    pub(crate) const fn volume(&self) -> Option<&VolumeCreateOptions> {
        self.volume.as_ref()
    }

    pub(crate) const fn root_credential(&self) -> &CredentialRecord {
        &self.root_credential
    }

    pub(crate) const fn policy_mount_target(&self) -> &'static str {
        POLICY_MOUNT_TARGET
    }

    pub(crate) const fn data_mount_target(&self) -> &'static str {
        DATA_MOUNT_TARGET
    }

    pub(crate) fn policy_file(&self, policy_name: &str) -> String {
        format!("{POLICY_MOUNT_TARGET}/{policy_name}.json")
    }
}

impl Debug for ObjectStoreSharedInstancePlan {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ObjectStoreSharedInstancePlan")
            .field("flavor", &self.flavor)
            .field("container", &self.container)
            .field("volume", &self.volume)
            .field("root_credential", &self.root_credential)
            .finish()
    }
}

fn metadata(
    options: &ObjectStoreSharedInstancePlanOptions,
    kind: ResourceKind,
    retention: RetentionClass,
    fingerprint: &str,
) -> Result<ManagedResourceMetadata, ObjectStorePlanError> {
    ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: options.installation_id.clone(),
        kind,
        project_id: None,
        compatibility_fingerprint: fingerprint.to_owned(),
        schema_version: options.schema_version,
        desired_revision: options.desired_revision.clone(),
        retention,
    })
    .map_err(|error| ObjectStorePlanError::new(error.to_string()))
}
