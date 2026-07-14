use super::{
    RegisterAcceptedV7AdaptersOptions, register_accepted_v7_logical_data_adapters,
    register_accepted_v7_named_volume_adapters, register_accepted_v7_project_wide_adapters,
    register_accepted_v7_recreated_adapters,
};
use crate::control_plane::engine::{
    CommandExecutor, ContainerLifecycle, ContainerVolumeArchive, HealthObserver,
    V7ContainerCommandExecutor, V7ContainerRetirement, V7ContainerVolumeArchive, VolumeManager,
};
use crate::control_plane::migration::V7MigrationAdapterRegistry;
use crate::control_plane::state::V7MigrationExecutionRecord;

/// Composes the complete strategy set selected by one accepted v7 plan.
pub(crate) fn register_accepted_v7_adapters<'operation, E>(
    registry: &mut V7MigrationAdapterRegistry<'operation>,
    execution: &V7MigrationExecutionRecord,
    options: RegisterAcceptedV7AdaptersOptions<'operation, E>,
) -> Result<usize, String>
where
    E: Clone
        + CommandExecutor
        + ContainerLifecycle
        + ContainerVolumeArchive
        + HealthObserver
        + V7ContainerCommandExecutor
        + V7ContainerRetirement
        + V7ContainerVolumeArchive
        + VolumeManager
        + Send
        + Sync
        + 'operation,
{
    let accepted = options.logical.accepted;
    if accepted.project_id() != options.named_volumes.accepted.project_id()
        || accepted.canonical_project_path()
            != options.named_volumes.accepted.canonical_project_path()
        || accepted.evidence_revision() != options.named_volumes.accepted.evidence_revision()
        || accepted.project_id() != options.project_wide.accepted.project_id()
        || accepted.canonical_project_path()
            != options.project_wide.accepted.canonical_project_path()
        || accepted.evidence_revision() != options.project_wide.accepted.evidence_revision()
    {
        return Err("accepted v7 adapter contexts do not share one immutable identity".to_owned());
    }

    let mut registered = register_accepted_v7_recreated_adapters(
        registry,
        execution,
        options.resources,
        options.logical_resources,
    )?;
    registered += register_accepted_v7_logical_data_adapters(registry, execution, options.logical)?;
    registered +=
        register_accepted_v7_named_volume_adapters(registry, execution, options.named_volumes)?;
    registered +=
        register_accepted_v7_project_wide_adapters(registry, execution, options.project_wide)?;
    if registered != execution.checkpoints().len() {
        return Err(format!(
            "accepted v7 plan selected {} checkpoints but only {registered} exact strategies were composed",
            execution.checkpoints().len()
        ));
    }

    Ok(registered)
}
