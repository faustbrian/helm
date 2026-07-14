use super::{
    SqlServerMigrationInstancePlanOptions, SqlServerPlanError, SqlServerSharedInstancePlanOptions,
};
use crate::control_plane::DnsLabel;
use crate::control_plane::engine::{
    ContainerCreateOptions, ContainerHealthCheck, ContainerRestartPolicy, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ResourceKind, RetentionClass, VolumeCreateOptions, VolumeMount,
};
use crate::control_plane::shared_infrastructure::{
    IsolationCapability, PersistenceMode, SharedInstancePlan,
};
use crate::control_plane::state::{CredentialLifecycle, CredentialRecord, CredentialRecordOptions};
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};
use std::time::Duration;

const DATA_MOUNT_TARGET: &str = "/var/opt/mssql";

/// Exact Engine resources for one SQL Server compatibility profile.
pub(crate) struct SqlServerSharedInstancePlan {
    container: ContainerCreateOptions,
    volume: Option<VolumeCreateOptions>,
    bootstrap_credential: CredentialRecord,
    sqlcmd_path: String,
}

impl SqlServerSharedInstancePlan {
    pub(crate) fn new(
        shared: &SharedInstancePlan,
        options: SqlServerSharedInstancePlanOptions,
    ) -> Result<Self, SqlServerPlanError> {
        let identity = validate_profile(shared)?;
        validate_runtime_options(
            options.accept_eula,
            options.bootstrap_secret.expose(),
            &options.sqlcmd_path,
        )?;
        let container_name = format!("stackctl-shared-{identity}");
        materialize(
            shared,
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
                bootstrap_credential: credential(
                    format!("shared/{identity}/sqlserver-bootstrap"),
                    None,
                    options.bootstrap_secret.expose(),
                ),
                sqlcmd_path: options.sqlcmd_path,
            },
        )
    }

    /// Materializes a separately owned target without changing normal sharing.
    pub(crate) fn new_migration_target(
        shared: &SharedInstancePlan,
        options: SqlServerMigrationInstancePlanOptions,
    ) -> Result<Self, SqlServerPlanError> {
        validate_profile(shared)?;
        if shared.profile().persistence() != PersistenceMode::Persistent {
            return Err(SqlServerPlanError::new(
                "SQL Server migration targets require persistent storage",
            ));
        }
        validate_runtime_options(
            options.accept_eula,
            options.bootstrap_secret.expose(),
            &options.sqlcmd_path,
        )?;
        let project_id = DnsLabel::new("project", &options.project_id)
            .map_err(|error| SqlServerPlanError::new(error.to_string()))?;
        let migration_id = DnsLabel::new("migration", &options.migration_id)
            .map_err(|error| SqlServerPlanError::new(error.to_string()))?;
        let container_name = format!("stackctl-migration-{}", migration_id.as_str());

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
                bootstrap_credential: credential(
                    format!("migration/{}/sqlserver-bootstrap", migration_id.as_str()),
                    Some(project_id.as_str().to_owned()),
                    options.bootstrap_secret.expose(),
                ),
                sqlcmd_path: options.sqlcmd_path,
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

    pub(crate) fn sqlcmd_path(&self) -> &str {
        &self.sqlcmd_path
    }

    pub(crate) const fn data_mount_target(&self) -> &'static str {
        DATA_MOUNT_TARGET
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
    sqlcmd_path: String,
}

fn validate_profile(shared: &SharedInstancePlan) -> Result<&str, SqlServerPlanError> {
    let profile = shared.profile();
    if !matches!(profile.implementation(), "sqlserver" | "mssql") {
        return Err(SqlServerPlanError::new(format!(
            "SQL Server plan cannot materialize implementation '{}'",
            profile.implementation()
        )));
    }
    if profile.isolation() != IsolationCapability::DatabaseAndRole {
        return Err(SqlServerPlanError::new(
            "SQL Server sharing requires database_and_role isolation",
        ));
    }
    if profile.platform_architecture().is_none() {
        return Err(SqlServerPlanError::new(
            "SQL Server compatibility profile requires a Linux platform",
        ));
    }
    profile
        .fingerprint()
        .as_str()
        .strip_prefix("sha256:")
        .ok_or_else(|| SqlServerPlanError::new("SQL Server compatibility fingerprint is malformed"))
}

fn validate_runtime_options(
    accept_eula: bool,
    bootstrap_secret: &str,
    sqlcmd_path: &str,
) -> Result<(), SqlServerPlanError> {
    if !accept_eula {
        return Err(SqlServerPlanError::new(
            "SQL Server requires explicit EULA acceptance",
        ));
    }
    validate_password(bootstrap_secret, "SA")?;
    if !sqlcmd_path.starts_with('/') || sqlcmd_path.contains('\0') {
        return Err(SqlServerPlanError::new(
            "SQL Server sqlcmd path must be an absolute Linux path without NUL bytes",
        ));
    }
    Ok(())
}

fn credential(id: String, project_id: Option<String>, secret: &str) -> CredentialRecord {
    CredentialRecord::new(CredentialRecordOptions {
        credential_id: id,
        project_id,
        service_id: "sqlserver".to_owned(),
        username: "sa".to_owned(),
        secret: secret.to_owned(),
        lifecycle: CredentialLifecycle::Active,
    })
}

fn materialize(
    shared: &SharedInstancePlan,
    options: InstanceMaterializationOptions,
) -> Result<SqlServerSharedInstancePlan, SqlServerPlanError> {
    let profile = shared.profile();
    let fingerprint = profile.fingerprint().as_str();
    let retention = match profile.persistence() {
        PersistenceMode::Persistent => RetentionClass::Persistent,
        PersistenceMode::Ephemeral => RetentionClass::Disposable,
    };
    let health_check = ContainerHealthCheck::new(
        vec![
            options.sqlcmd_path.clone(),
            "-C".to_owned(),
            "-S".to_owned(),
            "127.0.0.1".to_owned(),
            "-U".to_owned(),
            "sa".to_owned(),
            "-Q".to_owned(),
            "SET NOCOUNT ON; SELECT 1".to_owned(),
        ],
        Duration::from_secs(5),
        Duration::from_secs(3),
        Duration::from_secs(20),
        20,
    )
    .map_err(|error| SqlServerPlanError::new(error.to_string()))?;
    let container_metadata = metadata(&options, options.kind, retention, fingerprint)?;
    let edition = profile
        .immutable_settings()
        .get("edition")
        .cloned()
        .unwrap_or_else(|| "Developer".to_owned());
    let environment = BTreeMap::from([
        ("ACCEPT_EULA".to_owned(), "Y".to_owned()),
        ("MSSQL_PID".to_owned(), edition),
        (
            "MSSQL_SA_PASSWORD".to_owned(),
            options.bootstrap_credential.secret().to_owned(),
        ),
        (
            "SQLCMDPASSWORD".to_owned(),
            options.bootstrap_credential.secret().to_owned(),
        ),
    ]);
    let mut container = ContainerCreateOptions::new(
        &options.container_name,
        profile.image_digest(),
        container_metadata,
    )
    .and_then(|request| request.with_network(&options.network_name))
    .and_then(|request| {
        request.with_platform(
            profile
                .platform_architecture()
                .expect("validated SQL Server profile has a Linux platform"),
        )
    })
    .and_then(|request| request.with_environment(environment))
    .map_err(|error| SqlServerPlanError::new(error.to_string()))?
    .with_health_check(health_check)
    .with_restart_policy(ContainerRestartPolicy::UnlessStopped);
    let volume = if profile.persistence() == PersistenceMode::Persistent {
        let volume_metadata = metadata(&options, ResourceKind::Volume, retention, fingerprint)?;
        let volume = VolumeCreateOptions::new(&options.volume_name, volume_metadata)
            .map_err(|error| SqlServerPlanError::new(error.to_string()))?;
        let mount = VolumeMount::read_write(&options.volume_name, DATA_MOUNT_TARGET)
            .map_err(|error| SqlServerPlanError::new(error.to_string()))?;
        container = container.with_volume_mount(mount);
        Some(volume)
    } else {
        None
    };

    Ok(SqlServerSharedInstancePlan {
        container,
        volume,
        bootstrap_credential: options.bootstrap_credential,
        sqlcmd_path: options.sqlcmd_path,
    })
}

impl Debug for SqlServerSharedInstancePlan {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SqlServerSharedInstancePlan")
            .field("container", &self.container)
            .field("volume", &self.volume)
            .field("bootstrap_credential", &self.bootstrap_credential)
            .field("sqlcmd_path", &self.sqlcmd_path)
            .finish()
    }
}

pub(super) fn validate_password(password: &str, identity: &str) -> Result<(), SqlServerPlanError> {
    let valid = password.len() >= 8
        && password.chars().any(char::is_uppercase)
        && password.chars().any(char::is_lowercase)
        && password.chars().any(|character| character.is_ascii_digit())
        && !password.contains('\0');
    if !valid {
        return Err(SqlServerPlanError::new(format!(
            "SQL Server {identity} password must contain at least eight characters, uppercase, lowercase, and a digit"
        )));
    }

    Ok(())
}

fn metadata(
    options: &InstanceMaterializationOptions,
    kind: ResourceKind,
    retention: RetentionClass,
    fingerprint: &str,
) -> Result<ManagedResourceMetadata, SqlServerPlanError> {
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: options.installation_id.clone(),
        kind,
        project_id: options.project_id.clone(),
        compatibility_fingerprint: fingerprint.to_owned(),
        schema_version: options.schema_version,
        desired_revision: options.desired_revision.clone(),
        retention,
    })
    .map_err(|error| SqlServerPlanError::new(error.to_string()))?;
    match &options.resource_id {
        Some(resource_id) => metadata
            .with_resource_id(resource_id)
            .map_err(|error| SqlServerPlanError::new(error.to_string())),
        None => Ok(metadata),
    }
}
