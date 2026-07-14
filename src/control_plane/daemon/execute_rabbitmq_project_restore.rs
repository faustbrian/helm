use super::ProjectRestoreExecutionOptions;
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ContainerLifecycle, ContainerNetworkIsolation,
    ContainerVolumeArchive, NetworkDiscovery, ResourceKind, VolumeDiscovery,
    reconstruct_owned_container, reconstruct_owned_network, reconstruct_owned_volume,
};
use crate::control_plane::migration::{
    MigrationExecutionResult, RabbitMqBackupOptions, RabbitMqRestoreOptions, backup_rabbitmq_vhost,
    restore_rabbitmq_vhost,
};
use crate::control_plane::network::matches_global_network;
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, LogicalResourceRecord, RecoveryPointRecord,
    RecoveryPointRecordOptions, ResourceLifecycle, SqliteStateStore, StateStore,
};

/// Restores one vhost after durably recording its current topology and messages.
pub(crate) async fn execute_rabbitmq_project_restore<E>(
    engine: &mut E,
    options: &ProjectRestoreExecutionOptions,
) -> Result<MigrationExecutionResult, String>
where
    E: CommandExecutor
        + ContainerDiscovery
        + ContainerLifecycle
        + ContainerNetworkIsolation
        + ContainerVolumeArchive
        + NetworkDiscovery
        + VolumeDiscovery,
{
    validate_options(options)?;
    let mut store = SqliteStateStore::open(&options.state_database_path)
        .map_err(|error| format!("could not open RabbitMQ restore state: {error}"))?;
    let logical = one(
        store
            .logical_resources()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|logical| source_matches(logical, options))
            .collect(),
        "active RabbitMQ logical resource",
    )?;
    let credential = one(
        store
            .credentials()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|credential| tenant_credential_matches(credential, &logical))
            .collect(),
        "active RabbitMQ tenant credential",
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
        "selected RabbitMQ recovery point",
    )?;
    let container = owned_shared_container(engine, options).await?;
    let volume = owned_shared_volume(engine, &logical, options).await?;
    let network = owned_global_network(engine, options).await?;
    let network_alias = format!(
        "stackctl-shared-{}",
        options
            .operation
            .compatibility_fingerprint()
            .strip_prefix("sha256:")
            .ok_or_else(|| "RabbitMQ restore fingerprint is malformed".to_owned())?
    );
    let safety_recovery_id = format!("{}-pre-restore", options.operation.operation_id());
    if safety_recovery_id == recovery.recovery_point_id() {
        return Err("RabbitMQ restore safety recovery collides with the selected point".to_owned());
    }
    let safety_matches = store
        .recovery_points(options.operation.project_id())
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|point| point.recovery_point_id() == safety_recovery_id)
        .collect::<Vec<_>>();
    if !safety_matches.is_empty()
        && !matches!(safety_matches.as_slice(), [point] if recovery_matches(point, &logical))
    {
        return Err("RabbitMQ restore safety recovery does not match the active vhost".to_owned());
    }
    engine
        .disconnect_container_network(&container, &network)
        .await
        .map_err(|error| format!("RabbitMQ restore network isolation failed: {error}"))?;
    let outcome: Result<MigrationExecutionResult, String> = Box::pin(async {
        if safety_matches.is_empty() {
            let backup = backup_rabbitmq_vhost(
                engine,
                &container,
                &volume,
                &RabbitMqBackupOptions {
                    logical_resource: &logical,
                    credential: &credential,
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
            })
            .map_err(|error| error.to_string())?;
            store
                .record_recovery_point(&safety_recovery)
                .map_err(|error| error.to_string())?;
        }
        restore_rabbitmq_vhost(
            engine,
            &container,
            &volume,
            &RabbitMqRestoreOptions {
                recovery_point: &recovery,
                logical_resource: &logical,
                credential: &credential,
                installation_id: &options.installation_id,
                verified_at_unix_seconds: options.updated_at_unix_seconds,
                timeout: options.timeout,
            },
        )
        .await
        .map_err(|error| error.to_string())?;

        Ok(MigrationExecutionResult::Confirmed)
    })
    .await;
    let reconnect = engine
        .reconnect_container_network(&container, &network, &network_alias)
        .await
        .map_err(|error| format!("RabbitMQ restore network reconnect failed: {error}"));
    match (outcome, reconnect) {
        (Ok(result), Ok(())) => Ok(result),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(reconnect)) => Err(format!("{error}; {reconnect}")),
    }
}

async fn owned_global_network<E>(
    engine: &E,
    options: &ProjectRestoreExecutionOptions,
) -> Result<crate::control_plane::engine::OwnedNetwork, String>
where
    E: NetworkDiscovery,
{
    let matches = engine
        .discover_managed_networks()
        .await
        .map_err(|error| error.to_string())?
        .iter()
        .filter_map(|observed| {
            reconstruct_owned_network(observed, &options.installation_id, options.schema_version)
                .ok()
        })
        .filter(|network| matches_global_network(network, &options.installation_id))
        .collect::<Vec<_>>();

    one(matches, "owned Stackctl network")
}

async fn owned_shared_volume<E>(
    engine: &E,
    logical: &LogicalResourceRecord,
    options: &ProjectRestoreExecutionOptions,
) -> Result<crate::control_plane::engine::OwnedVolume, String>
where
    E: VolumeDiscovery,
{
    let matches = engine
        .discover_managed_volumes()
        .await
        .map_err(|error| error.to_string())?
        .iter()
        .filter_map(|observed| {
            reconstruct_owned_volume(observed, &options.installation_id, options.schema_version)
                .ok()
        })
        .filter(|volume| {
            volume.name() == logical.shared_resource_id()
                && volume.metadata().kind() == ResourceKind::Volume
                && volume.metadata().project_id().is_none()
                && volume.metadata().compatibility_fingerprint()
                    == options.operation.compatibility_fingerprint()
        })
        .collect::<Vec<_>>();

    one(matches, "owned RabbitMQ shared volume")
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

    one(matches, "owned RabbitMQ shared container")
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
    let identity = format!(
        "{}_{}",
        logical.project_id().replace('-', "_"),
        logical.service_id().replace('-', "_")
    );
    credential.credential_id() == logical.logical_resource_id()
        && credential.project_id() == Some(logical.project_id())
        && credential.service_id() == logical.service_id()
        && credential.username() == format!("st_{identity}")
        && credential.lifecycle() == CredentialLifecycle::Active
}

fn recovery_matches(point: &RecoveryPointRecord, logical: &LogicalResourceRecord) -> bool {
    point.project_id() == logical.project_id()
        && point.service_id() == logical.service_id()
        && point.logical_resource_id() == logical.logical_resource_id()
        && point.resource_kind() == logical.kind()
        && point.compatibility_fingerprint() == logical.compatibility_fingerprint()
}

fn validate_options(options: &ProjectRestoreExecutionOptions) -> Result<(), String> {
    if options.operation.kind() != "rabbitmq_vhost_user"
        || options.installation_id.is_empty()
        || options.schema_version == 0
        || !options.state_database_path.is_absolute()
        || !options.backup_root.is_absolute()
        || options.updated_at_unix_seconds < 0
        || options.timeout.is_zero()
        || options.shared_target()?.profile().implementation() != "rabbitmq"
        || options.shared_target()?.fingerprint().as_str()
            != options.operation.compatibility_fingerprint()
    {
        return Err("RabbitMQ restore execution options are incomplete or incompatible".to_owned());
    }

    Ok(())
}

fn one<T>(mut matches: Vec<T>, description: &str) -> Result<T, String> {
    match matches.len() {
        1 => matches
            .pop()
            .ok_or_else(|| format!("project restore lost its exact {description}")),
        0 => Err(format!("project restore found no exact {description}")),
        count => Err(format!(
            "project restore found {count} matches for {description}; refusing to guess"
        )),
    }
}
