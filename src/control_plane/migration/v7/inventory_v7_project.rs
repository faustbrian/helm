use super::{
    V7InventoryError, V7ProjectInventory, V7ProjectInventoryOptions, V7ProjectInventoryRequest,
};
use crate::control_plane::engine::LegacyContainerDiscovery;

/// Discovers legacy containers through the typed Engine boundary and inventories config.
pub(crate) async fn inventory_v7_project(
    discovery: &impl LegacyContainerDiscovery,
    request: V7ProjectInventoryRequest<'_>,
) -> Result<V7ProjectInventory, V7InventoryError> {
    let observed_containers = discovery
        .discover_v7_managed()
        .await
        .map_err(|error| V7InventoryError::new(format!("v7 Engine inventory failed: {error}")))?;

    V7ProjectInventory::new(V7ProjectInventoryOptions {
        project_id: request.project_id,
        canonical_project_path: request.canonical_project_path,
        source_revision: request.source_revision,
        config: request.config,
        observed_containers: &observed_containers,
    })
}
