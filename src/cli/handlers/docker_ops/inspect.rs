//! cli handlers docker ops inspect module.
//!
//! Contains inspect handler used by Helm command workflows.

use anyhow::Result;

use crate::{cli, config, docker};

pub(super) fn handle_inspect(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    format: Option<&str>,
    json: bool,
    size: bool,
    object_type: Option<&str>,
) -> Result<()> {
    let selected = cli::support::selected_services(config, service, kind, None)?;
    if json {
        let items: Vec<serde_json::Value> = selected
            .into_iter()
            .map(|svc| {
                let container_name = svc.container_name()?;
                let payload = docker::inspect_json(&container_name).ok_or_else(|| {
                    anyhow::anyhow!("Failed to inspect container '{container_name}'")
                })?;
                Ok(payload)
            })
            .collect::<Result<Vec<serde_json::Value>>>()?;
        println!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(());
    }

    for svc in selected {
        docker::inspect_container(svc, format, size, object_type)?;
    }
    Ok(())
}
