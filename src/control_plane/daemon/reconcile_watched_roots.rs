use super::{
    DiscoveryReconciliationError, DiscoveryReconciliationResult, ProjectDiscoveryOptions,
    discover_project_sources,
};
use crate::control_plane::application::ControlPlane;
use crate::control_plane::state::StateStore;

/// Scans every authoritative root and publishes only a complete valid registry.
pub(crate) fn reconcile_watched_roots<Store>(
    control_plane: &mut ControlPlane<Store>,
    options: ProjectDiscoveryOptions,
    orphaned_at_unix_seconds: i64,
) -> Result<DiscoveryReconciliationResult, DiscoveryReconciliationError>
where
    Store: StateStore,
{
    let roots = control_plane.watched_roots()?;
    let report = discover_project_sources(&roots, options)?;
    if !report.issues().is_empty() {
        return Ok(DiscoveryReconciliationResult::blocked(report));
    }

    let registry =
        control_plane.reconcile_discovered_projects(report.sources(), orphaned_at_unix_seconds)?;

    Ok(DiscoveryReconciliationResult::applied(report, registry))
}
