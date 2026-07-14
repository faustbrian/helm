use super::ProjectRestoreExecutionOptions;
use crate::control_plane::engine::{
    ContainerDiscovery, ContainerLifecycle, ContainerVolumeArchive, HealthObserver, ResourceKind,
    VolumeDiscovery, VolumeManager, reconstruct_owned_container, reconstruct_owned_volume,
};
use crate::control_plane::migration::{
    MigrationExecutionResult, ProjectVolumeBackupOptions, ProjectVolumeRestoreOptions,
    backup_project_volume, restore_project_volume,
};
use crate::control_plane::retention::verify_resource_recovery_point_artifact;
use crate::control_plane::state::{
    RecoveryPointRecord, RecoveryPointRecordOptions, ResourceLifecycle, ResourceRecord,
    ResourceRetention, SqliteStateStore, StateStore,
};

/// Restores one dedicated volume after cataloging its exact current contents.
pub(crate) async fn execute_project_volume_restore<E>(
    engine: &mut E,
    options: &ProjectRestoreExecutionOptions,
) -> Result<MigrationExecutionResult, String>
where
    E: ContainerDiscovery
        + ContainerLifecycle
        + ContainerVolumeArchive
        + HealthObserver
        + VolumeDiscovery
        + VolumeManager,
{
    validate_options(options)?;
    let mut store = SqliteStateStore::open(&options.state_database_path)
        .map_err(|error| format!("could not open project volume restore state: {error}"))?;
    let resource = one(
        store
            .resources()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|resource| source_matches(resource, options))
            .collect(),
        "active persistent volume",
    )?;
    let recovery = one(
        store
            .recovery_points(options.operation.project_id())
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|point| {
                point.recovery_point_id() == options.operation.recovery_point_id()
                    && recovery_matches(point, &resource)
            })
            .collect(),
        "selected project volume recovery point",
    )?;
    let container = owned_container(engine, options).await?;
    let volume = owned_volume(engine, options).await?;
    ensure_safety_recovery(
        &mut store, engine, &container, &volume, &resource, &recovery, options,
    )
    .await?;
    let target = options.dedicated_target()?;
    let desired_volume = target
        .volume()
        .ok_or_else(|| "project volume restore target has no retained volume".to_owned())?;
    restore_project_volume(
        engine,
        &container,
        &volume,
        &ProjectVolumeRestoreOptions {
            resource: &resource,
            recovery_point: &recovery,
            desired_container: target.request(),
            desired_volume,
            installation_id: &options.installation_id,
            project_id: options.operation.project_id(),
            service_id: options.operation.service_id(),
            verified_at_unix_seconds: options.updated_at_unix_seconds,
            timeout: options.timeout,
        },
    )
    .await
    .map_err(|error| error.to_string())?;

    Ok(MigrationExecutionResult::Confirmed)
}

async fn ensure_safety_recovery<E>(
    store: &mut SqliteStateStore,
    engine: &mut E,
    container: &crate::control_plane::engine::OwnedContainer,
    volume: &crate::control_plane::engine::OwnedVolume,
    resource: &ResourceRecord,
    selected: &RecoveryPointRecord,
    options: &ProjectRestoreExecutionOptions,
) -> Result<(), String>
where
    E: ContainerLifecycle + ContainerVolumeArchive,
{
    let recovery_id = format!("{}-pre-restore", options.operation.operation_id());
    if recovery_id == selected.recovery_point_id() {
        return Err(
            "project volume restore safety recovery collides with the selected point".to_owned(),
        );
    }
    let matches = store
        .recovery_points(options.operation.project_id())
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|point| point.recovery_point_id() == recovery_id)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => {
            let backup = backup_project_volume(
                engine,
                container,
                volume,
                &ProjectVolumeBackupOptions {
                    resource,
                    installation_id: &options.installation_id,
                    project_id: options.operation.project_id(),
                    service_id: options.operation.service_id(),
                    created_at_unix_seconds: options.updated_at_unix_seconds,
                    backup_root: &options.backup_root,
                    timeout: options.timeout,
                },
            )
            .await
            .map_err(|error| error.to_string())?;
            let safety = RecoveryPointRecord::new(RecoveryPointRecordOptions {
                recovery_point_id: recovery_id,
                project_id: resource.project_id().unwrap_or_default().to_owned(),
                service_id: resource.scope_id().unwrap_or_default().to_owned(),
                logical_resource_id: resource.resource_id().to_owned(),
                resource_kind: resource.kind().to_owned(),
                compatibility_fingerprint: resource.compatibility_fingerprint().to_owned(),
                reference: backup.reference().to_owned(),
                artifact_sha256: backup.artifact_sha256().to_owned(),
                artifact_size_bytes: backup.artifact_size_bytes(),
                created_at_unix_seconds: options.updated_at_unix_seconds,
                verified_at_unix_seconds: options.updated_at_unix_seconds,
            })
            .map_err(|error| error.to_string())?;
            store
                .record_recovery_point(&safety)
                .map_err(|error| error.to_string())
        }
        [safety] if recovery_matches(safety, resource) => verify_resource_recovery_point_artifact(
            safety,
            resource,
            options.updated_at_unix_seconds,
        )
        .map(|_| ())
        .map_err(|error| error.to_string()),
        [_] => Err(
            "project volume restore safety recovery does not match the active volume".to_owned(),
        ),
        _ => Err("project volume restore matched multiple safety recovery points".to_owned()),
    }
}

async fn owned_container<E>(
    engine: &E,
    options: &ProjectRestoreExecutionOptions,
) -> Result<crate::control_plane::engine::OwnedContainer, String>
where
    E: ContainerDiscovery,
{
    one(
        engine
            .discover_managed()
            .await
            .map_err(|error| error.to_string())?
            .iter()
            .filter_map(|observed| {
                reconstruct_owned_container(
                    observed,
                    &options.installation_id,
                    options.schema_version,
                )
                .ok()
            })
            .filter(|container| {
                container.metadata().kind() == ResourceKind::ProjectService
                    && container.metadata().project_id() == Some(options.operation.project_id())
                    && container.metadata().resource_id() == Some(options.operation.service_id())
                    && container.metadata().compatibility_fingerprint()
                        == options.operation.compatibility_fingerprint()
            })
            .collect(),
        "owned dedicated service container",
    )
}

async fn owned_volume<E>(
    engine: &E,
    options: &ProjectRestoreExecutionOptions,
) -> Result<crate::control_plane::engine::OwnedVolume, String>
where
    E: VolumeDiscovery,
{
    one(
        engine
            .discover_managed_volumes()
            .await
            .map_err(|error| error.to_string())?
            .iter()
            .filter_map(|observed| {
                reconstruct_owned_volume(observed, &options.installation_id, options.schema_version)
                    .ok()
            })
            .filter(|volume| {
                volume.name() == options.operation.logical_resource_id()
                    && volume.metadata().kind() == ResourceKind::Volume
                    && volume.metadata().project_id() == Some(options.operation.project_id())
                    && volume.metadata().resource_id() == Some(options.operation.service_id())
                    && volume.metadata().compatibility_fingerprint()
                        == options.operation.compatibility_fingerprint()
            })
            .collect(),
        "owned dedicated service volume",
    )
}

fn source_matches(resource: &ResourceRecord, options: &ProjectRestoreExecutionOptions) -> bool {
    resource.resource_id() == options.operation.logical_resource_id()
        && resource.project_id() == Some(options.operation.project_id())
        && resource.scope_id() == Some(options.operation.service_id())
        && resource.kind() == options.operation.kind()
        && resource.compatibility_fingerprint() == options.operation.compatibility_fingerprint()
        && resource.retention() == ResourceRetention::Persistent
        && resource.lifecycle() == ResourceLifecycle::Active
}

fn recovery_matches(recovery: &RecoveryPointRecord, resource: &ResourceRecord) -> bool {
    recovery.project_id() == resource.project_id().unwrap_or_default()
        && recovery.service_id() == resource.scope_id().unwrap_or_default()
        && recovery.logical_resource_id() == resource.resource_id()
        && recovery.resource_kind() == resource.kind()
        && recovery.compatibility_fingerprint() == resource.compatibility_fingerprint()
}

fn validate_options(options: &ProjectRestoreExecutionOptions) -> Result<(), String> {
    if options.operation.kind() != "volume"
        || options.installation_id.is_empty()
        || options.schema_version == 0
        || !options.state_database_path.is_absolute()
        || !options.backup_root.is_absolute()
        || options.updated_at_unix_seconds < 0
        || options.timeout.is_zero()
        || options
            .dedicated_target()?
            .request()
            .metadata()
            .compatibility_fingerprint()
            != options.operation.compatibility_fingerprint()
    {
        return Err(
            "project volume restore execution options are incomplete or incompatible".to_owned(),
        );
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
