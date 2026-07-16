use super::{
    MongoDbMigrationInstancePlanOptions, MongoDbPlanError, MongoDbSharedInstancePlanOptions,
};
use crate::control_plane::DnsLabel;
use crate::control_plane::engine::{
    BindMount, ContainerCreateOptions, ContainerRestartPolicy, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ResourceKind, RetentionClass, VolumeCreateOptions, VolumeMount,
};
use crate::control_plane::shared_infrastructure::{
    IsolationCapability, PersistenceMode, SharedInstancePlan, shared_container_name,
};
use crate::control_plane::state::{CredentialLifecycle, CredentialRecord, CredentialRecordOptions};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const DATA_MOUNT_TARGET: &str = "/data/db";
const BOOTSTRAP_SECRET_TARGET: &str = "/run/stackctl-secrets/mongodb-root-password";

/// Exact Engine resources for one MongoDB compatibility profile.
pub(crate) struct MongoDbSharedInstancePlan {
    container: ContainerCreateOptions,
    volume: Option<VolumeCreateOptions>,
    bootstrap_credential: CredentialRecord,
    bootstrap_secret_file: PathBuf,
}

impl MongoDbSharedInstancePlan {
    pub(crate) fn new(
        shared: &SharedInstancePlan,
        options: MongoDbSharedInstancePlanOptions,
    ) -> Result<Self, MongoDbPlanError> {
        let identity = validate_profile(shared)?;
        let container_name = shared_container_name(identity);
        let bootstrap_credential = CredentialRecord::new(CredentialRecordOptions {
            credential_id: format!("shared/{identity}/mongodb-bootstrap"),
            project_id: None,
            service_id: "mongodb".to_owned(),
            username: "stackctl_admin".to_owned(),
            secret: options.bootstrap_secret.expose().to_owned(),
            lifecycle: CredentialLifecycle::Active,
        });

        materialize(
            shared,
            InstanceMaterializationOptions {
                container_name: container_name.clone(),
                volume_name: format!("{container_name}-data"),
                installation_id: options.installation_id,
                project_id: None,
                resource_id: Some(container_name),
                kind: ResourceKind::SharedService,
                network_name: options.network_name,
                schema_version: options.schema_version,
                desired_revision: options.desired_revision,
                bootstrap_credential,
                bootstrap_secret_file: options.bootstrap_secret_file,
            },
        )
    }

    /// Materializes a separately owned target without changing normal sharing.
    pub(crate) fn new_migration_target(
        shared: &SharedInstancePlan,
        options: MongoDbMigrationInstancePlanOptions,
    ) -> Result<Self, MongoDbPlanError> {
        validate_profile(shared)?;
        if shared.profile().persistence() != PersistenceMode::Persistent {
            return Err(MongoDbPlanError::new(
                "MongoDB migration targets require persistent storage",
            ));
        }
        let project_id = DnsLabel::new("project", &options.project_id)
            .map_err(|error| MongoDbPlanError::new(error.to_string()))?;
        let migration_id = DnsLabel::new("migration", &options.migration_id)
            .map_err(|error| MongoDbPlanError::new(error.to_string()))?;
        let container_name = format!("stackctl-migration-{}", migration_id.as_str());
        let bootstrap_credential = CredentialRecord::new(CredentialRecordOptions {
            credential_id: format!("migration/{}/mongodb-bootstrap", migration_id.as_str()),
            project_id: Some(project_id.as_str().to_owned()),
            service_id: "mongodb".to_owned(),
            username: "stackctl_admin".to_owned(),
            secret: options.bootstrap_secret.expose().to_owned(),
            lifecycle: CredentialLifecycle::Active,
        });

        materialize(
            shared,
            InstanceMaterializationOptions {
                container_name: container_name.clone(),
                volume_name: format!("{container_name}-data"),
                installation_id: options.installation_id,
                project_id: Some(project_id.as_str().to_owned()),
                resource_id: Some(migration_id.as_str().to_owned()),
                kind: ResourceKind::ProjectService,
                network_name: options.network_name,
                schema_version: options.schema_version,
                desired_revision: options.desired_revision,
                bootstrap_credential,
                bootstrap_secret_file: options.bootstrap_secret_file,
            },
        )
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

    pub(crate) fn bootstrap_secret_file(&self) -> &Path {
        &self.bootstrap_secret_file
    }

    #[cfg(test)]
    pub(crate) const fn data_mount_target(&self) -> &'static str {
        DATA_MOUNT_TARGET
    }

    #[cfg(test)]
    pub(crate) const fn bootstrap_secret_target(&self) -> &'static str {
        BOOTSTRAP_SECRET_TARGET
    }
}

struct InstanceMaterializationOptions {
    container_name: String,
    volume_name: String,
    installation_id: String,
    project_id: Option<String>,
    resource_id: Option<String>,
    kind: ResourceKind,
    network_name: String,
    schema_version: u32,
    desired_revision: String,
    bootstrap_credential: CredentialRecord,
    bootstrap_secret_file: PathBuf,
}

fn validate_profile(shared: &SharedInstancePlan) -> Result<&str, MongoDbPlanError> {
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
    if profile.platform_architecture().is_none() {
        return Err(MongoDbPlanError::new(
            "MongoDB compatibility profile requires a Linux platform",
        ));
    }
    profile
        .fingerprint()
        .as_str()
        .strip_prefix("sha256:")
        .ok_or_else(|| MongoDbPlanError::new("MongoDB fingerprint is malformed"))
}

fn materialize(
    shared: &SharedInstancePlan,
    options: InstanceMaterializationOptions,
) -> Result<MongoDbSharedInstancePlan, MongoDbPlanError> {
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
    let profile = shared.profile();
    let fingerprint = profile.fingerprint().as_str();
    let retention = match profile.persistence() {
        PersistenceMode::Persistent => RetentionClass::Persistent,
        PersistenceMode::Ephemeral => RetentionClass::Disposable,
    };
    let platform = profile.platform_architecture().ok_or_else(|| {
        MongoDbPlanError::new("MongoDB compatibility profile has no Linux platform")
    })?;
    let secret_mount = BindMount::read_only(secret_file, BOOTSTRAP_SECRET_TARGET)
        .map_err(|error| MongoDbPlanError::new(error.to_string()))?;
    let container_metadata = metadata(&options, options.kind, retention, fingerprint)?
        .with_compatibility_profile(profile.implementation(), profile.major_version())
        .map_err(|error| MongoDbPlanError::new(error.to_string()))?;
    let mut container = ContainerCreateOptions::new(
        &options.container_name,
        profile.image_digest(),
        container_metadata,
    )
    .and_then(|request| request.with_network(&options.network_name))
    .and_then(|request| request.with_platform(platform))
    .and_then(|request| {
        request.with_environment(BTreeMap::from([
            (
                "MONGO_INITDB_ROOT_USERNAME".to_owned(),
                options.bootstrap_credential.username().to_owned(),
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
        let volume = VolumeCreateOptions::new(&options.volume_name, volume_metadata)
            .map_err(|error| MongoDbPlanError::new(error.to_string()))?;
        let mount = VolumeMount::read_write(&options.volume_name, DATA_MOUNT_TARGET)
            .map_err(|error| MongoDbPlanError::new(error.to_string()))?;
        container = container.with_volume_mount(mount);

        Some(volume)
    } else {
        None
    };

    Ok(MongoDbSharedInstancePlan {
        container,
        volume,
        bootstrap_credential: options.bootstrap_credential,
        bootstrap_secret_file: options.bootstrap_secret_file,
    })
}

fn metadata(
    options: &InstanceMaterializationOptions,
    kind: ResourceKind,
    retention: RetentionClass,
    fingerprint: &str,
) -> Result<ManagedResourceMetadata, MongoDbPlanError> {
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: options.installation_id.clone(),
        kind,
        project_id: options.project_id.clone(),
        compatibility_fingerprint: fingerprint.to_owned(),
        schema_version: options.schema_version,
        desired_revision: options.desired_revision.clone(),
        retention,
    })
    .map_err(|error| MongoDbPlanError::new(error.to_string()))?;
    match &options.resource_id {
        Some(resource_id) => metadata
            .with_resource_id(resource_id)
            .map_err(|error| MongoDbPlanError::new(error.to_string())),
        None => Ok(metadata),
    }
}
