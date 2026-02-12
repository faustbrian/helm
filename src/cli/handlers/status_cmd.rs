use anyhow::Result;

use crate::{cli, config, display, docker};

pub(crate) fn handle_status(
    config: &config::Config,
    format: &str,
    kind: Option<config::Kind>,
    driver: Option<config::Driver>,
) -> Result<()> {
    let filtered = cli::support::filter_services(&config.service, kind, driver);
    match format {
        "json" => {
            let statuses: Vec<serde_json::Value> = filtered
                .iter()
                .map(|svc| {
                    let status = svc
                        .container_name()
                        .ok()
                        .and_then(|name| docker::inspect_status(&name));
                    serde_json::json!({
                        "name": svc.name,
                        "kind": format!("{:?}", svc.kind).to_lowercase(),
                        "driver": format!("{:?}", svc.driver).to_lowercase(),
                        "image": svc.image,
                        "port": svc.port,
                        "status": status.unwrap_or_else(|| "not created".to_owned()),
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&statuses)?);
        }
        _ => display::print_status(&filtered),
    }
    Ok(())
}
