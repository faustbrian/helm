use crate::control_plane::migration::V7LogicalDataMigrationSource;
use crate::control_plane::shared_infrastructure::MySqlFlavor;
use crate::control_plane::state::AcceptedV7InventoryRecord;

/// Resolves MySQL versus MariaDB only from the exact accepted service image.
pub(super) fn accepted_v7_mysql_flavor(
    accepted: &AcceptedV7InventoryRecord,
    source: &V7LogicalDataMigrationSource,
) -> Result<MySqlFlavor, String> {
    let inventory = serde_json::from_str::<serde_json::Value>(accepted.inventory_json())
        .map_err(|error| format!("accepted v7 MySQL-family evidence is invalid: {error}"))?;
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
        return Err("accepted v7 inventory has ambiguous MySQL-family evidence".to_owned());
    }
    let image = matching[0]
        .get("configured_image")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "accepted v7 MySQL-family service has no configured image".to_owned())?;
    let repository = image
        .split_once('@')
        .map_or(image, |(repository, _)| repository);
    let last_slash = repository.rfind('/');
    let repository = repository
        .rfind(':')
        .filter(|colon| last_slash.is_none_or(|slash| *colon > slash))
        .map_or(repository, |colon| &repository[..colon]);
    let segments = repository.split('/').collect::<Vec<_>>();
    match (segments.contains(&"mysql"), segments.contains(&"mariadb")) {
        (true, false) => return Ok(MySqlFlavor::MySql),
        (false, true) => return Ok(MySqlFlavor::MariaDb),
        _ => {}
    }

    Err(format!(
        "accepted v7 MySQL-family image '{image}' has unknown implementation"
    ))
}
