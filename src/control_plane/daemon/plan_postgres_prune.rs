use super::ipc::IpcPostgresPrunePlan;
use crate::control_plane::application::ControlPlane;
use crate::control_plane::retention::{LogicalPrunePlan, LogicalPrunePlanOptions};
use crate::control_plane::state::StateStore;

/// Builds exact immutable logical prune intent without mutating durable state.
pub(crate) fn plan_postgres_prune<Store>(
    control_plane: &ControlPlane<Store>,
    project_id: &str,
    service_id: &str,
    recovery_point_id: &str,
) -> Result<IpcPostgresPrunePlan, String>
where
    Store: StateStore,
{
    build_postgres_prune_plan(control_plane, project_id, service_id, recovery_point_id)
        .map(|plan| IpcPostgresPrunePlan::from(&plan))
}

pub(crate) fn build_postgres_prune_plan<Store>(
    control_plane: &ControlPlane<Store>,
    project_id: &str,
    service_id: &str,
    recovery_point_id: &str,
) -> Result<LogicalPrunePlan, String>
where
    Store: StateStore,
{
    let installation = control_plane
        .installation()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "the v8 installation identity has not been initialized".to_owned())?;
    let projects = control_plane
        .projects()
        .map_err(|error| error.to_string())?;
    let logical_resources = control_plane
        .logical_resources()
        .map_err(|error| error.to_string())?;
    let credentials = control_plane
        .credentials()
        .map_err(|error| error.to_string())?;
    let recovery_points = control_plane
        .recovery_points(project_id)
        .map_err(|error| error.to_string())?;
    LogicalPrunePlan::new(LogicalPrunePlanOptions {
        installation_id: installation.installation_id(),
        project_id,
        service_id,
        recovery_point_id,
        project_registered: projects
            .iter()
            .any(|project| project.project_name() == project_id),
        logical_resources: &logical_resources,
        credentials: &credentials,
        recovery_points: &recovery_points,
    })
}
