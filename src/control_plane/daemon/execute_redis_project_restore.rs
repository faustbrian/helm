use super::ProjectRestoreExecutionOptions;
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ResourceKind, reconstruct_owned_container,
};
use crate::control_plane::migration::{
    MigrationExecutionResult, RedisBackupOptions, RedisRestoreOptions, backup_redis_prefix,
    restore_redis_prefix,
};
use crate::control_plane::shared_infrastructure::RedisFlavor;
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, LogicalResourceRecord, RecoveryPointRecord,
    RecoveryPointRecordOptions, ResourceLifecycle, SqliteStateStore, StateStore,
};

/// Restores one tenant prefix in place after durably recording its current state.
pub(crate) async fn execute_redis_project_restore<E>(
    engine: &E,
    options: &ProjectRestoreExecutionOptions,
) -> Result<MigrationExecutionResult, String>
where
    E: CommandExecutor + ContainerDiscovery,
{
    validate_options(options)?;
    let mut store = SqliteStateStore::open(&options.state_database_path)
        .map_err(|error| format!("could not open Redis restore state: {error}"))?;
    let logical = one(
        store
            .logical_resources()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|logical| source_matches(logical, options))
            .collect(),
        "active Redis-compatible logical resource",
    )?;
    let credentials = store.credentials().map_err(|error| error.to_string())?;
    let credential = one(
        credentials
            .iter()
            .filter(|credential| tenant_credential_matches(credential, &logical))
            .cloned()
            .collect(),
        "active Redis-compatible tenant credential",
    )?;
    let flavor = redis_flavor(logical.kind())?;
    let administrator = one(
        credentials
            .into_iter()
            .filter(|credential| administrator_matches(credential, &logical, flavor))
            .collect(),
        "active Redis-compatible shared administrator",
    )?;
    let recovery = one(
        store
            .recovery_points(options.operation.project_id())
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|point| {
                point.recovery_point_id() == options.operation.recovery_point_id()
                    && recovery_matches(point, &logical)
            })
            .collect(),
        "selected Redis-compatible recovery point",
    )?;
    let container = owned_shared_container(engine, options).await?;
    let prefix = format!(
        "stackctl:{}:{}:",
        logical.project_id(),
        logical.service_id()
    );
    let safety_recovery_id = format!("{}-pre-restore", options.operation.operation_id());
    if safety_recovery_id == recovery.recovery_point_id() {
        return Err("Redis restore safety recovery collides with the selected point".to_owned());
    }
    let safety_matches = store
        .recovery_points(options.operation.project_id())
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|point| point.recovery_point_id() == safety_recovery_id)
        .collect::<Vec<_>>();
    if safety_matches.is_empty() {
        let backup = backup_redis_prefix(
            engine,
            &container,
            &RedisBackupOptions {
                flavor,
                logical_resource: &logical,
                credential: &credential,
                administrator: &administrator,
                prefix: &prefix,
                installation_id: &options.installation_id,
                created_at_unix_seconds: options.updated_at_unix_seconds,
                backup_root: &options.backup_root,
                timeout: options.timeout,
            },
        )
        .await
        .map_err(|error| error.to_string())?;
        let safety_recovery = RecoveryPointRecord::new(RecoveryPointRecordOptions {
            recovery_point_id: safety_recovery_id,
            project_id: logical.project_id().to_owned(),
            service_id: logical.service_id().to_owned(),
            logical_resource_id: logical.logical_resource_id().to_owned(),
            resource_kind: logical.kind().to_owned(),
            compatibility_fingerprint: logical.compatibility_fingerprint().to_owned(),
            reference: backup.reference().to_owned(),
            artifact_sha256: backup.artifact_sha256().to_owned(),
            artifact_size_bytes: backup.artifact_size_bytes(),
            created_at_unix_seconds: options.updated_at_unix_seconds,
            verified_at_unix_seconds: options.updated_at_unix_seconds,
        })?;
        store
            .record_recovery_point(&safety_recovery)
            .map_err(|error| error.to_string())?;
    } else if !matches!(safety_matches.as_slice(), [point] if recovery_matches(point, &logical)) {
        return Err(
            "Redis restore safety recovery does not match the active tenant prefix".to_owned(),
        );
    }
    restore_redis_prefix(
        engine,
        &container,
        &RedisRestoreOptions {
            flavor,
            logical_resource: &logical,
            credential: &credential,
            administrator: &administrator,
            recovery_point: &recovery,
            prefix: &prefix,
            installation_id: &options.installation_id,
            restored_at_unix_seconds: options.updated_at_unix_seconds,
            timeout: options.timeout,
        },
    )
    .await
    .map_err(|error| error.to_string())?;

    Ok(MigrationExecutionResult::Confirmed)
}

async fn owned_shared_container<E>(
    engine: &E,
    options: &ProjectRestoreExecutionOptions,
) -> Result<crate::control_plane::engine::OwnedContainer, String>
where
    E: ContainerDiscovery,
{
    let matches = engine
        .discover_managed()
        .await
        .map_err(|error| error.to_string())?
        .iter()
        .filter_map(|observed| {
            reconstruct_owned_container(observed, &options.installation_id, options.schema_version)
                .ok()
        })
        .filter(|container| {
            container.metadata().kind() == ResourceKind::SharedService
                && container.metadata().project_id().is_none()
                && container.metadata().compatibility_fingerprint()
                    == options.operation.compatibility_fingerprint()
        })
        .collect::<Vec<_>>();

    one(matches, "owned Redis-compatible shared container")
}

fn source_matches(
    logical: &LogicalResourceRecord,
    options: &ProjectRestoreExecutionOptions,
) -> bool {
    logical.logical_resource_id() == options.operation.logical_resource_id()
        && logical.project_id() == options.operation.project_id()
        && logical.service_id() == options.operation.service_id()
        && logical.kind() == options.operation.kind()
        && logical.compatibility_fingerprint() == options.operation.compatibility_fingerprint()
        && logical.lifecycle() == ResourceLifecycle::Active
}

fn tenant_credential_matches(
    credential: &CredentialRecord,
    logical: &LogicalResourceRecord,
) -> bool {
    credential.credential_id() == logical.logical_resource_id()
        && credential.project_id() == Some(logical.project_id())
        && credential.service_id() == logical.service_id()
        && credential.lifecycle() == CredentialLifecycle::Active
}

fn administrator_matches(
    credential: &CredentialRecord,
    logical: &LogicalResourceRecord,
    flavor: RedisFlavor,
) -> bool {
    let fingerprint = logical
        .compatibility_fingerprint()
        .strip_prefix("sha256:")
        .unwrap_or_default();
    credential.credential_id()
        == format!("shared/{fingerprint}/{}-bootstrap", flavor.implementation())
        && credential.project_id().is_none()
        && credential.service_id() == flavor.implementation()
        && credential.username() == "stackctl_admin"
        && credential.lifecycle() == CredentialLifecycle::Active
}

fn recovery_matches(point: &RecoveryPointRecord, logical: &LogicalResourceRecord) -> bool {
    point.project_id() == logical.project_id()
        && point.service_id() == logical.service_id()
        && point.logical_resource_id() == logical.logical_resource_id()
        && point.resource_kind() == logical.kind()
        && point.compatibility_fingerprint() == logical.compatibility_fingerprint()
}

fn redis_flavor(kind: &str) -> Result<RedisFlavor, String> {
    match kind {
        "redis_acl_prefix" => Ok(RedisFlavor::Redis),
        "valkey_acl_prefix" => Ok(RedisFlavor::Valkey),
        _ => Err(format!(
            "restore kind '{kind}' is not a Redis-compatible resource"
        )),
    }
}

fn validate_options(options: &ProjectRestoreExecutionOptions) -> Result<(), String> {
    if options.installation_id.is_empty()
        || options.schema_version == 0
        || !options.state_database_path.is_absolute()
        || !options.backup_root.is_absolute()
        || options.updated_at_unix_seconds < 0
        || options.timeout.is_zero()
        || options.shared_target()?.fingerprint().as_str()
            != options.operation.compatibility_fingerprint()
    {
        return Err("Redis restore execution options are incomplete or incompatible".to_owned());
    }
    let flavor = redis_flavor(options.operation.kind())?;
    if options.shared_target()?.profile().implementation() != flavor.implementation() {
        return Err("Redis restore selected an incompatible shared service plan".to_owned());
    }

    Ok(())
}

fn one<T>(mut matches: Vec<T>, description: &str) -> Result<T, String> {
    match matches.len() {
        1 => Ok(matches.pop().expect("single match exists")),
        0 => Err(format!("project restore found no exact {description}")),
        count => Err(format!(
            "project restore found {count} matches for {description}; refusing to guess"
        )),
    }
}
