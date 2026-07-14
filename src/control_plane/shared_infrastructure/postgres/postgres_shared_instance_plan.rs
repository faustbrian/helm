use super::{
    POSTGRES_BOOTSTRAP_USERNAME, PostgresMigrationInstancePlanOptions, PostgresPlanError,
    PostgresSharedInstancePlanOptions,
};
use crate::control_plane::DnsLabel;
use crate::control_plane::engine::{
    ContainerCreateOptions, ContainerRestartPolicy, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ResourceKind, RetentionClass, VolumeCreateOptions, VolumeMount,
};
use crate::control_plane::shared_infrastructure::{
    CompatibilityProfile, IsolationCapability, PersistenceMode, SharedInstancePlan,
};
use crate::control_plane::state::{CredentialLifecycle, CredentialRecord, CredentialRecordOptions};
use std::collections::BTreeMap;

const POSTGRES_LEGACY_DATA_TARGET: &str = "/var/lib/postgresql/data";
const POSTGRES_CURRENT_DATA_TARGET: &str = "/var/lib/postgresql";

/// Exact Engine resources for one compatibility-keyed PostgreSQL process.
pub(crate) struct PostgresSharedInstancePlan {
    container: ContainerCreateOptions,
    volume: Option<VolumeCreateOptions>,
    data_mount_target: String,
    bootstrap_credential: CredentialRecord,
}

impl PostgresSharedInstancePlan {
    pub(crate) fn new(
        shared: &SharedInstancePlan,
        options: PostgresSharedInstancePlanOptions,
    ) -> Result<Self, PostgresPlanError> {
        let profile = validate_profile(shared)?;
        let fingerprint = profile.fingerprint().as_str();
        let identity = fingerprint.strip_prefix("sha256:").ok_or_else(|| {
            PostgresPlanError::new("PostgreSQL compatibility fingerprint is malformed")
        })?;
        let container_name = format!("stackctl-shared-{identity}");
        let bootstrap_credential = CredentialRecord::new(CredentialRecordOptions {
            credential_id: format!("shared/{identity}/postgresql-bootstrap"),
            project_id: None,
            service_id: "postgresql".to_owned(),
            username: POSTGRES_BOOTSTRAP_USERNAME.to_owned(),
            secret: options.bootstrap_secret.expose().to_owned(),
            lifecycle: CredentialLifecycle::Active,
        });
        materialize(
            profile,
            InstanceMaterializationOptions {
                container_name: container_name.clone(),
                volume_name: format!("{container_name}-data"),
                installation_id: options.installation_id,
                project_id: None,
                resource_id: None,
                kind: ResourceKind::SharedService,
                network_name: options.network_name,
                schema_version: options.schema_version,
                desired_revision: options.desired_revision,
                bootstrap_credential,
            },
        )
    }

    /// Materializes a separately owned target without changing normal sharing.
    pub(crate) fn new_migration_target(
        shared: &SharedInstancePlan,
        options: PostgresMigrationInstancePlanOptions,
    ) -> Result<Self, PostgresPlanError> {
        let profile = validate_profile(shared)?;
        let project_id = DnsLabel::new("project", &options.project_id)
            .map_err(|error| PostgresPlanError::new(error.to_string()))?;
        let migration_id = DnsLabel::new("migration", &options.migration_id)
            .map_err(|error| PostgresPlanError::new(error.to_string()))?;
        let container_name = format!("stackctl-migration-{}", migration_id.as_str());
        let bootstrap_credential = CredentialRecord::new(CredentialRecordOptions {
            credential_id: format!("migration/{}/postgresql-bootstrap", migration_id.as_str()),
            project_id: Some(project_id.as_str().to_owned()),
            service_id: "postgresql".to_owned(),
            username: POSTGRES_BOOTSTRAP_USERNAME.to_owned(),
            secret: options.bootstrap_secret.expose().to_owned(),
            lifecycle: CredentialLifecycle::Active,
        });

        materialize(
            profile,
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
            },
        )
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

    pub(crate) const fn bootstrap_credential(&self) -> &CredentialRecord {
        &self.bootstrap_credential
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
}

fn validate_profile(
    shared: &SharedInstancePlan,
) -> Result<&CompatibilityProfile, PostgresPlanError> {
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
    if profile.platform_architecture().is_none() {
        return Err(PostgresPlanError::new(
            "PostgreSQL compatibility profile requires a Linux platform",
        ));
    }

    Ok(profile)
}

fn materialize(
    profile: &CompatibilityProfile,
    options: InstanceMaterializationOptions,
) -> Result<PostgresSharedInstancePlan, PostgresPlanError> {
    let fingerprint = profile.fingerprint().as_str();
    let retention = match profile.persistence() {
        PersistenceMode::Persistent => RetentionClass::Persistent,
        PersistenceMode::Ephemeral => RetentionClass::Disposable,
    };
    let platform = profile.platform_architecture().ok_or_else(|| {
        PostgresPlanError::new("PostgreSQL compatibility profile has no Linux platform")
    })?;
    let container_metadata = metadata(&options, options.kind, retention, fingerprint)?;
    let mut container = ContainerCreateOptions::new(
        &options.container_name,
        profile.image_digest(),
        container_metadata,
    )
    .and_then(|request| request.with_network(&options.network_name))
    .and_then(|request| request.with_platform(platform))
    .and_then(|request| {
        request.with_environment(BTreeMap::from([
            ("POSTGRES_DB".to_owned(), "postgres".to_owned()),
            (
                "POSTGRES_USER".to_owned(),
                POSTGRES_BOOTSTRAP_USERNAME.to_owned(),
            ),
            (
                "POSTGRES_PASSWORD".to_owned(),
                options.bootstrap_credential.secret().to_owned(),
            ),
        ]))
    })
    .map_err(|error| PostgresPlanError::new(error.to_string()))?
    .with_restart_policy(ContainerRestartPolicy::UnlessStopped);
    let data_mount_target = data_mount_target(profile.major_version(), profile)?;
    let volume = if profile.persistence() == PersistenceMode::Persistent {
        let volume_metadata = metadata(&options, ResourceKind::Volume, retention, fingerprint)?;
        let volume = VolumeCreateOptions::new(&options.volume_name, volume_metadata)
            .map_err(|error| PostgresPlanError::new(error.to_string()))?;
        let mount = VolumeMount::read_write(&options.volume_name, &data_mount_target)
            .map_err(|error| PostgresPlanError::new(error.to_string()))?;
        container = container.with_volume_mount(mount);
        Some(volume)
    } else {
        None
    };

    Ok(PostgresSharedInstancePlan {
        container,
        volume,
        data_mount_target,
        bootstrap_credential: options.bootstrap_credential,
    })
}

fn metadata(
    options: &InstanceMaterializationOptions,
    kind: ResourceKind,
    retention: RetentionClass,
    fingerprint: &str,
) -> Result<ManagedResourceMetadata, PostgresPlanError> {
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: options.installation_id.clone(),
        kind,
        project_id: options.project_id.clone(),
        compatibility_fingerprint: fingerprint.to_owned(),
        schema_version: options.schema_version,
        desired_revision: options.desired_revision.clone(),
        retention,
    })
    .map_err(|error| PostgresPlanError::new(error.to_string()))?;
    match &options.resource_id {
        Some(resource_id) => metadata
            .with_resource_id(resource_id)
            .map_err(|error| PostgresPlanError::new(error.to_string())),
        None => Ok(metadata),
    }
}

fn data_mount_target(
    major_version: &str,
    profile: &CompatibilityProfile,
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
