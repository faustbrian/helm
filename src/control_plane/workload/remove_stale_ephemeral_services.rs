use super::WorkloadReconcileError;
use crate::control_plane::engine::{
    ContainerLifecycle, ContainerState, EngineError, ObservedContainer, ObservedResourceOwnership,
    ResourceKind, reconstruct_owned_container,
};

/// Removes stale browser sessions against a pass-wide Engine observation.
pub(crate) async fn remove_stale_ephemeral_services_from_observed<E>(
    engine: &mut E,
    observed: &[ObservedContainer],
    installation_id: &str,
    schema_version: u32,
) -> Result<usize, WorkloadReconcileError>
where
    E: ContainerLifecycle,
{
    let mut removed = 0;

    for observed in observed {
        let container = match reconstruct_owned_container(observed, installation_id, schema_version)
        {
            Ok(container) if container.metadata().kind() == ResourceKind::EphemeralService => {
                container
            }
            Ok(_) | Err(ObservedResourceOwnership::Unmanaged) => continue,
            Err(ObservedResourceOwnership::ForeignInstallation { .. }) => continue,
            Err(ownership) => {
                return Err(WorkloadReconcileError::Conflict {
                    detail: format!(
                        "managed container '{}' has invalid ownership while recovering ephemeral services: {ownership:?}",
                        observed.id().as_str()
                    ),
                });
            }
        };

        match engine
            .inspect(&container)
            .await
            .map_err(|error| engine_error("inspect stale ephemeral service", error))?
        {
            ContainerState::Running => engine
                .stop(&container)
                .await
                .map_err(|error| engine_error("stop stale ephemeral service", error))?,
            ContainerState::Stopped => {}
            ContainerState::Missing => continue,
        }
        engine
            .remove(&container)
            .await
            .map_err(|error| engine_error("remove stale ephemeral service", error))?;
        removed += 1;
    }

    Ok(removed)
}

fn engine_error(action: &str, error: EngineError) -> WorkloadReconcileError {
    WorkloadReconcileError::Engine {
        action: action.to_owned(),
        detail: error.to_string(),
    }
}
