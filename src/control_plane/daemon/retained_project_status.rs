use crate::control_plane::application::ControlPlane;
use crate::control_plane::daemon::ipc::{
    IpcDataLifecycle, IpcProjectStatus, IpcResourceHealth, IpcResourceLifecycle, IpcResourceStatus,
};
use crate::control_plane::state::{ResourceLifecycle, StateStore};
use std::collections::BTreeMap;

/// Projects retained resources without requiring an active registry row.
pub(crate) fn retained_project_status<Store>(
    control_plane: &ControlPlane<Store>,
) -> Result<Vec<IpcProjectStatus>, String>
where
    Store: StateStore,
{
    let mut projects = BTreeMap::<String, Vec<IpcResourceStatus>>::new();

    for resource in control_plane
        .resources()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|resource| resource.lifecycle() != ResourceLifecycle::Active)
    {
        let Some(project_id) = resource.project_id() else {
            continue;
        };
        projects.entry(project_id.to_owned()).or_default().push(
            IpcResourceStatus::new(
                resource
                    .scope_id()
                    .unwrap_or_else(|| resource.resource_id())
                    .to_owned(),
                resource.kind().to_owned(),
                ipc_resource_lifecycle(resource.lifecycle()),
                IpcResourceHealth::Unknown,
                None,
                false,
            )
            .with_orphaned_at_unix_seconds(
                resource.orphaned_at_unix_seconds().ok_or_else(|| {
                    "retained physical resource has no orphan timestamp".to_owned()
                })?,
            ),
        );
    }

    for resource in control_plane
        .logical_resources()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|resource| resource.lifecycle() != ResourceLifecycle::Active)
    {
        let data_lifecycle =
            match crate::control_plane::retention::resolve_data_lifecycle_strategy(&resource) {
                Ok(_) => IpcDataLifecycle::LogicalResource,
                Err(
                    crate::control_plane::retention::DataLifecycleStrategyError::NonAuthoritative {
                        ..
                    },
                ) => IpcDataLifecycle::None,
                Err(error) => return Err(error.to_string()),
            };
        projects
            .entry(resource.project_id().to_owned())
            .or_default()
            .push(
                IpcResourceStatus::with_data_lifecycle(
                    resource.service_id().to_owned(),
                    resource.kind().to_owned(),
                    ipc_resource_lifecycle(resource.lifecycle()),
                    IpcResourceHealth::Unknown,
                    None,
                    true,
                    data_lifecycle,
                )
                .with_orphaned_at_unix_seconds(
                    resource.orphaned_at_unix_seconds().ok_or_else(|| {
                        "retained logical resource has no orphan timestamp".to_owned()
                    })?,
                ),
            );
    }

    Ok(projects
        .into_iter()
        .map(|(project_id, mut resources)| {
            resources.sort_by(|left, right| {
                left.service()
                    .cmp(right.service())
                    .then_with(|| left.kind().cmp(right.kind()))
                    .then_with(|| left.shared().cmp(&right.shared()))
            });
            IpcProjectStatus::new(project_id, Vec::new(), resources)
        })
        .collect())
}

const fn ipc_resource_lifecycle(lifecycle: ResourceLifecycle) -> IpcResourceLifecycle {
    match lifecycle {
        ResourceLifecycle::Active => IpcResourceLifecycle::Active,
        ResourceLifecycle::Orphaned => IpcResourceLifecycle::Orphaned,
        ResourceLifecycle::Retained => IpcResourceLifecycle::Retained,
    }
}
