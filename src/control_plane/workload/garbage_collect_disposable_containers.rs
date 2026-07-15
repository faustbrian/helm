use super::{
    DisposableContainerGarbageCollectionOptions, WorkloadReconcileError,
    matches_durable_resource_metadata,
};
#[cfg(test)]
use crate::control_plane::engine::ContainerDiscovery;
use crate::control_plane::engine::{
    ContainerLifecycle, ContainerState, EngineError, ObservedContainer, ObservedResourceOwnership,
    ResourceKind, reconstruct_owned_container,
};
use crate::control_plane::retention::{DeletionDecision, PruneAuthorization, evaluate_deletion};
use crate::control_plane::state::ResourceRecord;

/// Removes only expired disposable containers with exact state and label proof.
#[cfg(test)]
pub(crate) async fn garbage_collect_disposable_containers<E>(
    engine: &mut E,
    options: DisposableContainerGarbageCollectionOptions<'_>,
) -> Result<Vec<ResourceRecord>, WorkloadReconcileError>
where
    E: ContainerDiscovery + ContainerLifecycle,
{
    let observed = engine
        .discover_managed()
        .await
        .map_err(|error| engine_error("discover disposable containers", error))?;

    garbage_collect_disposable_containers_from_observed(engine, &observed, options).await
}

/// Collects disposable containers against a pass-wide Engine observation.
pub(crate) async fn garbage_collect_disposable_containers_from_observed<E>(
    engine: &mut E,
    observed: &[ObservedContainer],
    options: DisposableContainerGarbageCollectionOptions<'_>,
) -> Result<Vec<ResourceRecord>, WorkloadReconcileError>
where
    E: ContainerLifecycle,
{
    let mut retired = Vec::new();

    for resource in options.resources {
        if evaluate_deletion(
            resource,
            options.now_unix_seconds,
            options.orphan_retention_seconds,
            PruneAuthorization::None,
        ) != DeletionDecision::DeleteDisposable
            || !is_container_kind(resource.kind())
        {
            continue;
        }
        let Some(observed) = observed
            .iter()
            .find(|observed| observed.id().as_str() == resource.resource_id())
        else {
            retired.push(resource.clone());
            continue;
        };
        let owned =
            reconstruct_owned_container(observed, options.installation_id, options.schema_version)
                .map_err(|ownership| ownership_error(resource, ownership))?;
        if !matches_durable_resource_metadata(resource, owned.metadata()) {
            return Err(WorkloadReconcileError::Conflict {
                detail: format!(
                    "disposable container '{}' differs from its durable ownership record",
                    resource.resource_id()
                ),
            });
        }

        match engine
            .inspect(&owned)
            .await
            .map_err(|error| engine_error("inspect disposable container", error))?
        {
            ContainerState::Running => engine
                .stop(&owned)
                .await
                .map_err(|error| engine_error("stop disposable container", error))?,
            ContainerState::Stopped => {}
            ContainerState::Missing => {
                retired.push(resource.clone());
                continue;
            }
        }
        engine
            .remove(&owned)
            .await
            .map_err(|error| engine_error("remove disposable container", error))?;
        retired.push(resource.clone());
    }

    Ok(retired)
}

fn is_container_kind(label: &str) -> bool {
    matches!(
        ResourceKind::from_label(label),
        Some(
            ResourceKind::ProjectApplication
                | ResourceKind::ProjectProcess
                | ResourceKind::ProjectService
                | ResourceKind::EphemeralService
                | ResourceKind::SharedService
                | ResourceKind::Gateway
                | ResourceKind::ProvisioningJob
        )
    )
}

fn ownership_error(
    resource: &ResourceRecord,
    ownership: ObservedResourceOwnership,
) -> WorkloadReconcileError {
    WorkloadReconcileError::Conflict {
        detail: format!(
            "disposable container '{}' has invalid ownership: {ownership:?}",
            resource.resource_id()
        ),
    }
}

fn engine_error(action: &str, error: EngineError) -> WorkloadReconcileError {
    WorkloadReconcileError::Engine {
        action: action.to_owned(),
        detail: error.to_string(),
    }
}
