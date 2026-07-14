use super::ipc::{IpcV7Mount, IpcV7ProjectInventory, IpcV7ServiceInventory};
use crate::control_plane::migration::V7NamedVolumeMigrationSource;
use crate::control_plane::state::{AcceptedV7InventoryRecord, V7MigrationExecutionRecord};

/// Resolves exact named-volume sources only from immutable accepted evidence.
pub(crate) fn resolve_accepted_v7_named_volume_sources(
    accepted: &AcceptedV7InventoryRecord,
    execution: &V7MigrationExecutionRecord,
) -> Result<Vec<V7NamedVolumeMigrationSource>, String> {
    if accepted.project_id() != execution.project_id()
        || accepted.canonical_project_path() != execution.canonical_project_path()
        || accepted.evidence_revision() != execution.evidence_revision()
    {
        return Err("accepted v7 named-volume composition identity is inconsistent".to_owned());
    }
    let inventory = serde_json::from_str::<IpcV7ProjectInventory>(accepted.inventory_json())
        .map_err(|error| format!("accepted v7 named-volume inventory is invalid: {error}"))?;
    if inventory.project_id() != accepted.project_id()
        || inventory.canonical_project_path() != accepted.canonical_project_path()
        || inventory.source_revision() != accepted.source_revision()
    {
        return Err("accepted v7 named-volume inventory identity is inconsistent".to_owned());
    }

    execution
        .checkpoints()
        .iter()
        .filter(|checkpoint| checkpoint.adapter_kind() == "named-volume-archive")
        .map(|checkpoint| {
            let service_id = checkpoint
                .adapter_id()
                .strip_prefix("volume/")
                .filter(|service_id| !service_id.is_empty() && !service_id.contains('/'))
                .ok_or_else(|| {
                    format!(
                        "accepted v7 named-volume adapter '{}' has no exact service identity",
                        checkpoint.adapter_id()
                    )
                })?;
            let service = one_service(inventory.services(), service_id)?;
            let container_id = service.observed_container_id().ok_or_else(|| {
                format!(
                    "accepted v7 named-volume service '{service_id}' has no exact Engine container"
                )
            })?;
            let configured = named_volumes(service.configured_mounts(), service_id)?;
            let observed = named_volumes(service.observed_mounts(), service_id)?;
            if configured != observed {
                return Err(format!(
                    "accepted v7 named-volume service '{service_id}' configured and observed volumes differ"
                ));
            }
            V7NamedVolumeMigrationSource::new(service_id, container_id, configured)
        })
        .collect()
}

fn one_service<'inventory>(
    services: &'inventory [IpcV7ServiceInventory],
    service_id: &str,
) -> Result<&'inventory IpcV7ServiceInventory, String> {
    let matching = services
        .iter()
        .filter(|service| service.service_id() == service_id)
        .collect::<Vec<_>>();
    match matching.as_slice() {
        [service] => Ok(*service),
        [] => Err(format!(
            "accepted v7 named-volume service '{service_id}' is missing"
        )),
        _ => Err(format!(
            "accepted v7 named-volume service '{service_id}' is ambiguous"
        )),
    }
}

fn named_volumes(mounts: &[IpcV7Mount], service_id: &str) -> Result<Vec<String>, String> {
    let mut names = mounts
        .iter()
        .filter(|mount| mount.source_kind() == "named_volume")
        .map(|mount| mount.source().to_owned())
        .collect::<Vec<_>>();
    names.sort();
    if names.is_empty()
        || names.iter().any(String::is_empty)
        || names.windows(2).any(|pair| pair[0] == pair[1])
    {
        return Err(format!(
            "accepted v7 named-volume service '{service_id}' volume identity is invalid"
        ));
    }

    Ok(names)
}
