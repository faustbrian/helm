use super::V7LogicalDataMigrationSource;
use crate::control_plane::state::AcceptedV7InventoryRecord;

/// Rebinds every logical-data command and resource identity to accepted evidence.
pub(crate) fn validate_v7_logical_data_migration_source(
    accepted: &AcceptedV7InventoryRecord,
    source: &V7LogicalDataMigrationSource,
) -> Result<(), String> {
    if accepted.project_id() != source.project_id() {
        return Err("legacy logical-data project differs from accepted v7 evidence".to_owned());
    }
    let inventory = serde_json::from_str::<serde_json::Value>(accepted.inventory_json())
        .map_err(|error| format!("accepted v7 logical-data evidence is invalid: {error}"))?;
    let services = inventory
        .get("services")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "accepted v7 inventory has no service evidence".to_owned())?;
    let matching = services
        .iter()
        .filter(|service| {
            service
                .get("service_id")
                .and_then(serde_json::Value::as_str)
                == Some(source.service_id())
        })
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err("accepted v7 inventory has ambiguous logical-data service evidence".to_owned());
    }
    let service = matching[0];
    if service.get("kind").and_then(serde_json::Value::as_str) != Some(source.kind())
        || service
            .get("container_name")
            .and_then(serde_json::Value::as_str)
            != Some(source.container_name())
    {
        return Err(
            "legacy logical-data command target differs from accepted v7 evidence".to_owned(),
        );
    }
    if service.get("driver").and_then(serde_json::Value::as_str) != Some(source.driver())
        || service
            .get("observed_container_id")
            .and_then(serde_json::Value::as_str)
            != Some(source.container_id())
    {
        return Err("legacy logical-data source differs from accepted v7 evidence".to_owned());
    }
    let logical_data = service
        .get("logical_data")
        .cloned()
        .ok_or_else(|| "accepted v7 service has no logical-data evidence".to_owned())?;
    let logical_data =
        serde_json::from_value::<std::collections::BTreeMap<String, String>>(logical_data)
            .map_err(|error| format!("accepted v7 logical-data identity is invalid: {error}"))?;
    if &logical_data != source.logical_data() {
        return Err("legacy logical-data identity differs from accepted v7 evidence".to_owned());
    }

    Ok(())
}
