use super::{OrphanedProjectWorkloadOptions, WorkloadReconcileError};
use crate::control_plane::engine::{
    ContainerDiscovery, ContainerLifecycle, ContainerState, EngineError, ManagedResourceMetadata,
    ObservedResourceOwnership, ResourceKind, RetentionClass, reconstruct_owned_container,
};
use crate::control_plane::state::{ResourceLifecycle, ResourceRecord, ResourceRetention};

/// Stops state-proven removed project workloads while retaining their containers.
pub(crate) async fn stop_orphaned_project_workloads<E>(
    engine: &mut E,
    options: OrphanedProjectWorkloadOptions<'_>,
) -> Result<usize, WorkloadReconcileError>
where
    E: ContainerDiscovery + ContainerLifecycle,
{
    let observed = engine
        .discover_managed()
        .await
        .map_err(|error| engine_error("discover orphaned project workloads", error))?;
    let mut stopped = 0;

    for container in &observed {
        let owned = match reconstruct_owned_container(
            container,
            options.installation_id,
            options.schema_version,
        ) {
            Ok(owned)
                if matches!(
                    owned.metadata().kind(),
                    ResourceKind::ProjectApplication | ResourceKind::ProjectProcess
                ) =>
            {
                owned
            }
            Ok(_) | Err(ObservedResourceOwnership::Unmanaged) => continue,
            Err(ObservedResourceOwnership::ForeignInstallation { .. }) => continue,
            Err(ownership) => {
                return Err(WorkloadReconcileError::Conflict {
                    detail: format!(
                        "managed container '{}' has invalid ownership while stopping orphaned project workloads: {ownership:?}",
                        container.id().as_str()
                    ),
                });
            }
        };
        let Some(resource) = options
            .resources
            .iter()
            .find(|resource| resource.resource_id() == owned.id().as_str())
        else {
            continue;
        };
        if !matches_durable_ownership(resource, owned.metadata()) {
            return Err(WorkloadReconcileError::Conflict {
                detail: format!(
                    "project workload '{}' differs from its durable ownership record",
                    owned.id().as_str()
                ),
            });
        }
        if resource.lifecycle() == ResourceLifecycle::Active {
            continue;
        }

        match engine
            .inspect(&owned)
            .await
            .map_err(|error| engine_error("inspect orphaned project workload", error))?
        {
            ContainerState::Running => {
                engine
                    .stop(&owned)
                    .await
                    .map_err(|error| engine_error("stop orphaned project workload", error))?;
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
}

const fn retention(retention: RetentionClass) -> ResourceRetention {
    match retention {
        RetentionClass::Persistent => ResourceRetention::Persistent,
        RetentionClass::Disposable => ResourceRetention::Disposable,
        RetentionClass::BuildCache => ResourceRetention::BuildCache,
    }
}

fn engine_error(action: &str, error: EngineError) -> WorkloadReconcileError {
    WorkloadReconcileError::Engine {
        action: action.to_owned(),
        detail: error.to_string(),
    }
}
