use super::RegisterAcceptedV7NamedVolumeAdaptersOptions;
use crate::control_plane::engine::{
    ContainerLifecycle, ContainerVolumeArchive, HealthObserver, OwnedContainer, OwnedVolume,
    V7ContainerRetirement, V7ContainerVolumeArchive, VolumeManager,
};
use crate::control_plane::migration::{
    V7MigrationAdapterRegistry, V7NamedVolumeMigrationAdapterOptions,
    V7NamedVolumeMigrationProvider, V7NamedVolumeMigrationProviderOptions,
    V7NamedVolumeMigrationSource, register_v7_named_volume_migration_adapter,
};
use crate::control_plane::state::V7MigrationExecutionRecord;
use crate::control_plane::workload::DedicatedProjectServicePlan;

/// Binds every accepted-v7 named volume to one exact prepared-v8 target.
pub(crate) fn register_accepted_v7_named_volume_adapters<'operation, E>(
    registry: &mut V7MigrationAdapterRegistry<'operation>,
    execution: &V7MigrationExecutionRecord,
    options: RegisterAcceptedV7NamedVolumeAdaptersOptions<'operation, E>,
) -> Result<usize, String>
where
    E: Clone
        + ContainerLifecycle
        + ContainerVolumeArchive
        + HealthObserver
        + V7ContainerRetirement
        + V7ContainerVolumeArchive
        + VolumeManager
        + Send
        + Sync
        + 'operation,
{
    if options.accepted.project_id() != execution.project_id()
        || options.accepted.canonical_project_path() != execution.canonical_project_path()
        || options.accepted.evidence_revision() != execution.evidence_revision()
    {
        return Err("accepted v7 named-volume registration identity is inconsistent".to_owned());
    }
    let mut registered = 0;
    for source in options.sources {
        let plan = one_plan(source, &options)?;
        let container = one_container(source, plan, &options)?;
        let volume = one_volume(source, plan, &options)?;
        let provider = V7NamedVolumeMigrationProvider::new(
            options.engine.clone(),
            V7NamedVolumeMigrationProviderOptions {
                accepted: options.accepted,
                source,
                target_container: container,
                target_volume: volume,
                target_plan: plan,
                backup_root: options.backup_root,
                created_at_unix_seconds: options.created_at_unix_seconds,
                verified_at_unix_seconds: options.verified_at_unix_seconds,
                timeout: options.timeout,
            },
        )
        .map_err(|error| error.to_string())?;
        if register_v7_named_volume_migration_adapter(
            registry,
            execution,
            V7NamedVolumeMigrationAdapterOptions {
                accepted: options.accepted,
                source,
                provider: Box::new(provider),
            },
        )? {
            registered += 1;
        }
    }

    Ok(registered)
}

fn one_plan<'operation, E>(
    source: &V7NamedVolumeMigrationSource,
    options: &'operation RegisterAcceptedV7NamedVolumeAdaptersOptions<'_, E>,
) -> Result<&'operation DedicatedProjectServicePlan, String> {
    one(
        options
            .reconciliation
            .dedicated_services()
            .iter()
            .filter(|plan| {
                plan.request().metadata().project_id() == Some(options.accepted.project_id())
                    && plan.request().metadata().resource_id() == Some(source.service_id())
                    && plan.volume().is_some()
            })
            .collect(),
        source,
        "reconciliation plan",
    )
}

fn one_container<'operation, E>(
    source: &V7NamedVolumeMigrationSource,
    plan: &DedicatedProjectServicePlan,
    options: &'operation RegisterAcceptedV7NamedVolumeAdaptersOptions<'_, E>,
) -> Result<&'operation OwnedContainer, String> {
    one(
        options
            .target_containers
            .iter()
            .filter(|container| container.metadata() == plan.request().metadata())
            .collect(),
        source,
        "owned container",
    )
}

fn one_volume<'operation, E>(
    source: &V7NamedVolumeMigrationSource,
    plan: &DedicatedProjectServicePlan,
    options: &'operation RegisterAcceptedV7NamedVolumeAdaptersOptions<'_, E>,
) -> Result<&'operation OwnedVolume, String> {
    let desired = plan
        .volume()
        .ok_or_else(|| "prepared v8 named-volume plan has no retained volume".to_owned())?;
    one(
        options
            .target_volumes
            .iter()
            .filter(|volume| {
                volume.name() == desired.name() && volume.metadata() == desired.metadata()
            })
            .collect(),
        source,
        "owned volume",
    )
}

fn one<'operation, T>(
    values: Vec<&'operation T>,
    source: &V7NamedVolumeMigrationSource,
    kind: &str,
) -> Result<&'operation T, String> {
    match values.as_slice() {
        [value] => Ok(*value),
        [] => Err(format!(
            "accepted v7 named-volume service '{}' exact v8 {kind} is missing",
            source.service_id()
        )),
        _ => Err(format!(
            "accepted v7 named-volume service '{}' exact v8 {kind} is ambiguous",
            source.service_id()
        )),
    }
}
