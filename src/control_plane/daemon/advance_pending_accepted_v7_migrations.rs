use super::ipc::IpcV7ProjectInventory;
use super::{
    AcceptedV7AdvanceReport, AdvanceAcceptedV7MigrationOptions,
    AdvancePendingAcceptedV7MigrationsOptions, RegisterAcceptedV7AdaptersOptions,
    RegisterAcceptedV7LogicalDataAdaptersOptions, RegisterAcceptedV7NamedVolumeAdaptersOptions,
    RegisterAcceptedV7ProjectWideAdaptersOptions, advance_accepted_v7_migration,
    resolve_accepted_v7_logical_data_inputs, resolve_accepted_v7_named_volume_sources,
};
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ContainerLifecycle, ContainerVolumeArchive,
    HealthObserver, V7ContainerCommandExecutor, V7ContainerRetirement, V7ContainerVolumeArchive,
    VolumeDiscovery, VolumeManager, reconstruct_owned_container, reconstruct_owned_volume,
};
use crate::control_plane::gateway::GatewaySnapshot;
use crate::control_plane::migration::{
    MigrationCutoverPlan, V7GatewaySnapshotMigrationAdapterOptions,
    V7InstallationTrustMigrationAdapterOptions, V7PublicFileArtifact,
};
use crate::control_plane::state::{StateStore, V7MigrationExecutionPhase};

/// Advances every non-terminal accepted v7 project without cross-project failure coupling.
pub(crate) async fn advance_pending_accepted_v7_migrations<Store, E>(
    mut options: AdvancePendingAcceptedV7MigrationsOptions<'_, Store, E>,
) -> Result<AcceptedV7AdvanceReport, String>
where
    Store: StateStore,
    E: Clone
        + CommandExecutor
        + ContainerDiscovery
        + ContainerLifecycle
        + ContainerVolumeArchive
        + HealthObserver
        + V7ContainerCommandExecutor
        + V7ContainerRetirement
        + V7ContainerVolumeArchive
        + VolumeDiscovery
        + VolumeManager
        + Send
        + Sync,
{
    validate(&options)?;
    let installation_id = options.installation_id;
    let schema_version = options.schema_version;
    let containers = options
        .engine
        .discover_managed()
        .await
        .map_err(|error| format!("discover accepted-v7 target containers: {error}"))?
        .iter()
        .filter_map(|observed| {
            reconstruct_owned_container(observed, installation_id, schema_version).ok()
        })
        .collect::<Vec<_>>();
    let volumes = options
        .engine
        .discover_managed_volumes()
        .await
        .map_err(|error| format!("discover accepted-v7 target volumes: {error}"))?
        .iter()
        .filter_map(|observed| {
            reconstruct_owned_volume(observed, installation_id, schema_version).ok()
        })
        .collect::<Vec<_>>();
    let resources = options
        .control_plane
        .resources()
        .map_err(|error| error.to_string())?;
    let logical_resources = options
        .control_plane
        .logical_resources()
        .map_err(|error| error.to_string())?;
    let environments = options
        .control_plane
        .managed_environments()
        .map_err(|error| error.to_string())?;
    let projects = options
        .control_plane
        .projects()
        .map_err(|error| error.to_string())?;
    let executions = options
        .control_plane
        .v7_migration_executions()
        .map_err(|error| error.to_string())?;
    let mut advanced = 0;
    let mut issues = Vec::new();

    for execution in executions.into_iter().filter(|execution| {
        matches!(
            execution.phase(),
            V7MigrationExecutionPhase::Planned
                | V7MigrationExecutionPhase::Preparing
                | V7MigrationExecutionPhase::Prepared
        )
    }) {
        let outcome = advance_one(
            &mut options,
            &execution,
            &containers,
            &volumes,
            &resources,
            &logical_resources,
            &environments,
            &projects,
        )
        .await;
        match outcome {
            Ok(()) => advanced += 1,
            Err(error) => issues.push(format!("project '{}': {error}", execution.project_id())),
        }
    }

    Ok(AcceptedV7AdvanceReport::new(advanced, issues))
}

#[expect(
    clippy::too_many_arguments,
    reason = "private loop body receives already-loaded immutable reconciliation snapshots"
)]
async fn advance_one<Store, E>(
    options: &mut AdvancePendingAcceptedV7MigrationsOptions<'_, Store, E>,
    execution: &crate::control_plane::state::V7MigrationExecutionRecord,
    containers: &[crate::control_plane::engine::OwnedContainer],
    volumes: &[crate::control_plane::engine::OwnedVolume],
    resources: &[crate::control_plane::state::ResourceRecord],
    logical_resources: &[crate::control_plane::state::LogicalResourceRecord],
    environments: &[crate::control_plane::state::ManagedEnvironmentRecord],
    projects: &[crate::control_plane::state::ProjectRecord],
) -> Result<(), String>
where
    Store: StateStore,
    E: Clone
        + CommandExecutor
        + ContainerDiscovery
        + ContainerLifecycle
        + ContainerVolumeArchive
        + HealthObserver
        + V7ContainerCommandExecutor
        + V7ContainerRetirement
        + V7ContainerVolumeArchive
        + VolumeDiscovery
        + VolumeManager
        + Send
        + Sync,
{
    let accepted = options
        .control_plane
        .accepted_v7_inventory(
            execution.canonical_project_path(),
            execution.evidence_revision(),
        )
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "accepted source evidence is missing".to_owned())?;
    let inputs = resolve_accepted_v7_logical_data_inputs(&accepted, options.maximum_config_bytes)?;
    let named_volumes = resolve_accepted_v7_named_volume_sources(&accepted, execution)?;
    let project = one(
        projects
            .iter()
            .filter(|project| {
                project.project_name() == execution.project_id()
                    && project.canonical_path() == execution.canonical_project_path()
            })
            .collect(),
        "registered project",
    )?;
    let environment = one(
        environments
            .iter()
            .filter(|environment| environment.project_id() == execution.project_id())
            .collect(),
        "active managed environment",
    )?;
    let desired_state = MigrationCutoverPlan::new(project.clone(), environment.clone())
        .map_err(|error| error.to_string())?;
    let rollback_gateway = GatewaySnapshot::new(
        options
            .target_gateway
            .routes()
            .iter()
            .filter(|route| {
                !project
                    .route_domains()
                    .iter()
                    .any(|domain| domain == route.domain())
            })
            .cloned()
            .collect(),
    )
    .map_err(|error| error.to_string())?;
    let inventory = serde_json::from_str::<IpcV7ProjectInventory>(accepted.inventory_json())
        .map_err(|error| format!("decode accepted host artifacts: {error}"))?;
    let legacy_ca_artifacts = inventory
        .host_artifacts()
        .caddy_ca_certificates()
        .iter()
        .map(|artifact| {
            V7PublicFileArtifact::new(
                artifact.path().to_path_buf(),
                artifact.revision().to_owned(),
                artifact.size_bytes(),
            )
        })
        .collect::<Vec<_>>();
    let gateway = execution
        .checkpoints()
        .iter()
        .any(|checkpoint| checkpoint.adapter_id() == "route")
        .then(|| V7GatewaySnapshotMigrationAdapterOptions {
            provider: &mut *options.gateway_provider,
            rollback_snapshot: &rollback_gateway,
            target_snapshot: options.target_gateway,
            backup_root: options.backup_root,
            created_at_unix_seconds: options.updated_at_unix_seconds,
            verified_at_unix_seconds: options.updated_at_unix_seconds,
        });
    let trust =
        (!legacy_ca_artifacts.is_empty()).then(|| V7InstallationTrustMigrationAdapterOptions {
            accepted: &accepted,
            legacy_ca_artifacts: &legacy_ca_artifacts,
            target_certificate_path: options.target_certificate_path,
            trust_store: options.trust_store,
            backup_root: options.backup_root,
            created_at_unix_seconds: options.updated_at_unix_seconds,
            verified_at_unix_seconds: options.updated_at_unix_seconds,
        });
    let advanced = advance_accepted_v7_migration(
        &mut *options.control_plane,
        AdvanceAcceptedV7MigrationOptions {
            execution,
            registration: RegisterAcceptedV7AdaptersOptions {
                resources,
                logical_resources,
                logical: RegisterAcceptedV7LogicalDataAdaptersOptions {
                    accepted: &accepted,
                    inputs: &inputs,
                    prepared: options.prepared_shared,
                    target_containers: containers,
                    logical_resources,
                    engine: options.engine,
                    installation_id: options.installation_id,
                    backup_root: options.backup_root,
                    created_at_unix_seconds: options.updated_at_unix_seconds,
                    verified_at_unix_seconds: options.updated_at_unix_seconds,
                    timeout: options.timeout,
                },
                named_volumes: RegisterAcceptedV7NamedVolumeAdaptersOptions {
                    accepted: &accepted,
                    sources: &named_volumes,
                    reconciliation: options.reconciliation,
                    target_containers: containers,
                    target_volumes: volumes,
                    engine: options.engine,
                    backup_root: options.backup_root,
                    created_at_unix_seconds: options.updated_at_unix_seconds,
                    verified_at_unix_seconds: options.updated_at_unix_seconds,
                    timeout: options.timeout,
                },
                project_wide: RegisterAcceptedV7ProjectWideAdaptersOptions {
                    accepted: &accepted,
                    gateway,
                    trust,
                    managed_environments: environments,
                    verified_at_unix_seconds: options.updated_at_unix_seconds,
                    maximum_environment_bytes: options.maximum_config_bytes,
                },
            },
            desired_state: &desired_state,
            updated_at_unix_seconds: options.updated_at_unix_seconds,
        },
    )
    .await
    .map_err(|error| error.to_string())?;
    if advanced.phase() != V7MigrationExecutionPhase::Cutover {
        return Err("automatic accepted-v7 advance did not reach cutover".to_owned());
    }

    Ok(())
}

fn validate<Store, E>(
    options: &AdvancePendingAcceptedV7MigrationsOptions<'_, Store, E>,
) -> Result<(), String>
where
    Store: StateStore,
{
    if options.installation_id.is_empty()
        || options.schema_version == 0
        || !options.backup_root.is_absolute()
        || !options.target_certificate_path.is_absolute()
        || options.maximum_config_bytes == 0
        || options.updated_at_unix_seconds < 0
        || options.timeout.is_zero()
    {
        return Err("pending accepted-v7 reconciliation options are invalid".to_owned());
    }
    Ok(())
}

fn one<'value, T>(values: Vec<&'value T>, kind: &str) -> Result<&'value T, String> {
    match values.as_slice() {
        [value] => Ok(*value),
        [] => Err(format!("accepted v7 {kind} is missing")),
        _ => Err(format!("accepted v7 {kind} is ambiguous")),
    }
}
