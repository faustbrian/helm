use super::ipc::IpcV7ProjectInventory;
use crate::control_plane::migration::{
    V7MigrationAdapterPlan, V7MigrationAdapterSelectionOptions, V7MigrationRouteSource,
    V7MigrationServiceSource, select_v7_migration_adapters,
};
use crate::control_plane::state::AcceptedV7InventoryRecord;

/// Resolves adapters only from immutable inventory already accepted by the daemon.
pub(crate) fn select_accepted_v7_migration_adapters(
    accepted: &AcceptedV7InventoryRecord,
) -> Result<V7MigrationAdapterPlan, String> {
    let inventory = serde_json::from_str::<IpcV7ProjectInventory>(accepted.inventory_json())
        .map_err(|error| format!("accepted v7 inventory cannot select adapters: {error}"))?;
    if inventory.project_id() != accepted.project_id()
        || inventory.canonical_project_path() != accepted.canonical_project_path()
        || inventory.source_revision() != accepted.source_revision()
        || !inventory.ready_for_automatic_migration()
    {
        return Err(
            "accepted v7 inventory identity or blocker evidence changed before adapter selection"
                .to_owned(),
        );
    }

    let named_volumes = inventory
        .services()
        .iter()
        .map(|service| {
            if let Some(mount) = service
                .configured_mounts()
                .iter()
                .find(|mount| mount.source_kind() != "named_volume")
            {
                return Err(format!(
                    "accepted v7 service '{}' mount '{}:{}' has unsupported source kind '{}'",
                    service.service_id(),
                    mount.source(),
                    mount.target(),
                    mount.source_kind()
                ));
            }
            Ok(service
                .configured_mounts()
                .iter()
                .filter(|mount| mount.source_kind() == "named_volume")
                .map(|mount| mount.source())
                .collect::<Vec<_>>())
        })
        .collect::<Result<Vec<_>, String>>()?;
    let services = inventory
        .services()
        .iter()
        .zip(&named_volumes)
        .map(|(service, volumes)| V7MigrationServiceSource {
            service_id: service.service_id(),
            driver: service.driver(),
            named_volumes: volumes,
        })
        .collect::<Vec<_>>();
    let routes = inventory
        .routes()
        .iter()
        .map(|route| V7MigrationRouteSource {
            service_id: route.service_id(),
            domain: route.domain(),
            scheme: route.scheme(),
            host_port: route.host_port(),
        })
        .collect::<Vec<_>>();
    let host_artifacts = inventory.host_artifacts();

    select_v7_migration_adapters(V7MigrationAdapterSelectionOptions {
        evidence_revision: accepted.evidence_revision(),
        services: &services,
        routes: &routes,
        requires_legacy_ca_capture: inventory.requires_legacy_ca_capture(),
        captured_ca_certificates: host_artifacts.caddy_ca_certificates().len(),
        generated_environment_present: host_artifacts.generated_environment().is_some(),
        protected_generated_environment: accepted.generated_environment_rollback().is_some(),
    })
    .map_err(|error| error.to_string())
}
