use super::{
    AcceptedV7MigrationAction, AdvanceAcceptedV7MigrationOptions,
    ExecuteAcceptedV7MigrationOptions, execute_accepted_v7_migration,
    register_accepted_v7_adapters,
};
use crate::control_plane::application::ControlPlane;
use crate::control_plane::engine::{
    CommandExecutor, ContainerLifecycle, ContainerVolumeArchive, HealthObserver,
    V7ContainerCommandExecutor, V7ContainerRetirement, V7ContainerVolumeArchive, VolumeManager,
};
use crate::control_plane::migration::{V7MigrationAdapterRegistry, V7MigrationExecutionError};
use crate::control_plane::state::{StateStore, V7MigrationExecutionRecord};

/// Composes and advances one accepted project through the atomic cutover barrier.
pub(crate) async fn advance_accepted_v7_migration<Store, E>(
    control_plane: &mut ControlPlane<Store>,
    options: AdvanceAcceptedV7MigrationOptions<'_, E>,
) -> Result<V7MigrationExecutionRecord, V7MigrationExecutionError>
where
    Store: StateStore,
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
        + Sync,
{
    let mut registry = V7MigrationAdapterRegistry::default();
    register_accepted_v7_adapters(&mut registry, options.execution, options.registration)
        .map_err(|detail| V7MigrationExecutionError::InvalidPlan { detail })?;
    execute_accepted_v7_migration(ExecuteAcceptedV7MigrationOptions {
        journal: control_plane,
        plan: options.execution,
        registry: &mut registry,
        action: AcceptedV7MigrationAction::PrepareAndCutover {
            desired_state: options.desired_state,
        },
        updated_at_unix_seconds: options.updated_at_unix_seconds,
    })
    .await
}
