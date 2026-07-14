/// Returns the exact sorted named-volume identities recorded for one v7 service.
pub(crate) fn accepted_v7_named_volumes(
    service: &serde_json::Value,
    field: &str,
) -> Result<Vec<String>, String> {
    let mut volumes = service
        .get(field)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("accepted v7 service has no {field} evidence"))?
        .iter()
        .filter(|mount| {
            mount.get("source_kind").and_then(serde_json::Value::as_str) == Some("named_volume")
        })
        .map(|mount| {
            mount
                .get("source")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| "accepted v7 named-volume identity is invalid".to_owned())
        })
        .collect::<Result<Vec<_>, String>>()?;
    volumes.sort();
    if volumes.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("accepted v7 named-volume identities are duplicated".to_owned());
    }

    Ok(volumes)
}
