use anyhow::Result;

use crate::{cli, config};

pub(crate) fn handle_list(
    config: &config::Config,
    format: &str,
    kind: Option<config::Kind>,
    driver: Option<config::Driver>,
) -> Result<()> {
    let filtered = cli::support::filter_services(&config.service, kind, driver);
    match format {
        "json" => {
            let names: Vec<&str> = filtered.iter().map(|svc| svc.name.as_str()).collect();
            println!("{}", serde_json::to_string_pretty(&names)?);
        }
        _ => {
            for svc in filtered {
                println!("{}", svc.name);
            }
        }
    }
    Ok(())
}
