use anyhow::Result;
use colored::Colorize;

use crate::{cli, config};

pub(crate) fn handle_url(
    config: &config::Config,
    service: Option<&str>,
    format: &str,
    kind: Option<config::Kind>,
    driver: Option<config::Driver>,
) -> Result<()> {
    let filtered = cli::support::filter_services(&config.service, kind, driver);
    match format {
        "json" => {
            let urls: Vec<serde_json::Value> = if let Some(name) = service {
                let svc = config::find_service(config, name)?;
                if cli::support::matches_filter(svc, kind, driver) {
                    vec![serde_json::json!({"name": svc.name, "url": svc.connection_url()})]
                } else {
                    Vec::new()
                }
            } else {
                filtered
                    .iter()
                    .map(|svc| serde_json::json!({"name": svc.name, "url": svc.connection_url()}))
                    .collect()
            };
            println!("{}", serde_json::to_string_pretty(&urls)?);
        }
        _ => {
            if let Some(name) = service {
                let svc = config::find_service(config, name)?;
                if cli::support::matches_filter(svc, kind, driver) {
                    println!("{}", svc.connection_url());
                }
            } else {
                for svc in filtered {
                    println!("{}: {}", svc.name.bold(), svc.connection_url());
                }
            }
        }
    }
    Ok(())
}
