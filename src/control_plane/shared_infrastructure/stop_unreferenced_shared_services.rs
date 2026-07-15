use super::{SharedInfrastructureReconcileError, UnreferencedSharedServiceOptions};
#[cfg(test)]
use crate::control_plane::engine::ContainerDiscovery;
use crate::control_plane::engine::{
    ContainerLifecycle, ContainerState, EngineError, ManagedResourceMetadata, ObservedContainer,
    ObservedResourceOwnership, ResourceKind, RetentionClass, reconstruct_owned_container,
};
use crate::control_plane::state::{ResourceLifecycle, ResourceRecord, ResourceRetention};

/// Stops unused shared processes while preserving their containers and volumes.
#[cfg(test)]
pub(crate) async fn stop_unreferenced_shared_services<Engine>(
    engine: &mut Engine,
    options: UnreferencedSharedServiceOptions<'_>,
) -> Result<usize, SharedInfrastructureReconcileError>
where
    Engine: ContainerDiscovery + ContainerLifecycle,
{
    let observed = engine
        .discover_managed()
        .await
        .map_err(|error| engine_error("discover shared services", error))?;

    stop_unreferenced_shared_services_from_observed(engine, &observed, options).await
}

/// Stops unused shared services against a pass-wide Engine observation.
pub(crate) async fn stop_unreferenced_shared_services_from_observed<Engine>(
    engine: &mut Engine,
    observed: &[ObservedContainer],
    options: UnreferencedSharedServiceOptions<'_>,
) -> Result<usize, SharedInfrastructureReconcileError>
where
    Engine: ContainerLifecycle,
{
    let mut stopped = 0;

    for container in observed {
        let owned = match reconstruct_owned_container(
            container,
            options.installation_id,
            options.schema_version,
        ) {
            Ok(owned) if owned.metadata().kind() == ResourceKind::SharedService => owned,
            Ok(_) | Err(ObservedResourceOwnership::Unmanaged) => continue,
            Err(ObservedResourceOwnership::ForeignInstallation { .. }) => continue,
            Err(ownership) => {
                return Err(SharedInfrastructureReconcileError::Conflict {
                    detail: format!(
                        "managed container '{}' has invalid ownership while idling shared services: {ownership:?}",
                        container.id().as_str()
                    ),
                });
            }
        };
        let resource = options
            .resources
            .iter()
            .find(|resource| resource.resource_id() == owned.id().as_str())
            .ok_or_else(|| SharedInfrastructureReconcileError::Conflict {
                detail: format!(
                    "shared service '{}' has no durable ownership record",
                    owned.id().as_str()
                ),
            })?;
        if !matches_durable_ownership(resource, owned.metadata()) {
            return Err(SharedInfrastructureReconcileError::Conflict {
                detail: format!(
                    "shared service '{}' differs from its durable ownership record",
                    owned.id().as_str()
                ),
            });
        }
        if options.logical_resources.iter().any(|logical| {
            logical.lifecycle() == ResourceLifecycle::Active
                && logical.compatibility_fingerprint()
                    == owned.metadata().compatibility_fingerprint()
        }) {
            continue;
        }

        match engine
            .inspect(&owned)
            .await
            .map_err(|error| engine_error("inspect unreferenced shared service", error))?
        {
            ContainerState::Running => {
                engine
                    .stop(&owned)
                    .await
                    .map_err(|error| engine_error("stop unreferenced shared service", error))?;
                stopped += 1;
            }
            ContainerState::Stopped | ContainerState::Missing => {}
        }
    }

    Ok(stopped)
}

fn matches_durable_ownership(
    resource: &ResourceRecord,
    metadata: &ManagedResourceMetadata,
) -> bool {
    resource.installation_id() == metadata.installation_id()
        && resource.kind() == metadata.kind().label()
        && resource.scope_id() == metadata.resource_id()
        && resource.compatibility_fingerprint() == metadata.compatibility_fingerprint()
        && resource.project_id() == metadata.project_id()
        && resource.schema_version() == metadata.schema_version()
        && resource.desired_revision() == metadata.desired_revision()
        && resource.retention() == retention(metadata.retention())
        && resource.lifecycle() == ResourceLifecycle::Active
}

const fn retention(retention: RetentionClass) -> ResourceRetention {
    match retention {
        RetentionClass::Persistent => ResourceRetention::Persistent,
        RetentionClass::Disposable => ResourceRetention::Disposable,
        RetentionClass::BuildCache => ResourceRetention::BuildCache,
    }
}

fn engine_error(action: &str, error: EngineError) -> SharedInfrastructureReconcileError {
    SharedInfrastructureReconcileError::Engine {
        action: action.to_owned(),
        detail: error.to_string(),
    }
}
