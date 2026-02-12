use anyhow::Result;

use crate::{cli, config};

pub(crate) fn handle_profile_list() {
    for name in cli::support::profile_names() {
        println!("{name}");
    }
}

pub(crate) fn handle_profile_show(config: &config::Config, name: &str, format: &str) -> Result<()> {
    let targets = cli::support::resolve_profile_targets(config, name)?;
    match format {
        "json" => {
            let values: Vec<serde_json::Value> = targets
                .iter()
                .map(|svc| {
                    serde_json::json!({
                        "name": svc.name,
                        "kind": format!("{:?}", svc.kind).to_lowercase(),
                        "driver": format!("{:?}", svc.driver).to_lowercase(),
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&values)?);
        }
        "markdown" => {
            println!("| service | kind | driver |");
            println!("|---|---|---|");
            for svc in targets {
                println!(
                    "| {} | {} | {} |",
                    svc.name,
                    format!("{:?}", svc.kind).to_lowercase(),
                    format!("{:?}", svc.driver).to_lowercase()
                );
            }
        }
        _ => {
            for svc in targets {
                println!(
                    "{}\t{}\t{}",
                    svc.name,
                    format!("{:?}", svc.kind).to_lowercase(),
                    format!("{:?}", svc.driver).to_lowercase()
                );
            }
        }
    }
    Ok(())
}
