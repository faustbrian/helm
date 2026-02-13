//! cli handlers open cmd module.
//!
//! Contains cli handlers open cmd logic used by Helm command workflows.

use anyhow::Result;

use crate::output::{self, LogLevel, Persistence};
use crate::{cli, config, docker};

/// Handles the `open` CLI command.
pub(crate) fn handle_open(
    config: &config::Config,
    service: Option<&str>,
    all: bool,
    health_path: Option<&str>,
    no_browser: bool,
    json: bool,
) -> Result<()> {
    let targets = if all {
        cli::support::app_services(config)
    } else {
        vec![config::resolve_app_service(config, service)?]
    };

    if targets.is_empty() {
        anyhow::bail!("no app services configured");
    }

    if json {
        let app_summaries: Vec<serde_json::Value> = targets
            .iter()
            .map(|target| cli::support::build_open_summary_json(target, health_path))
            .collect::<Result<Vec<_>>>()?;
        let databases: Vec<serde_json::Value> = config
            .service
            .iter()
            .filter(|svc| svc.kind == config::Kind::Database)
            .map(|svc| {
                let status = svc
                    .container_name()
                    .ok()
                    .and_then(|name| docker::inspect_status(&name))
                    .unwrap_or_else(|| "not created".to_owned());
                serde_json::json!({
                    "name": svc.name,
                    "status": status,
                    "url": svc.connection_url(),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "apps": app_summaries,
                "databases": databases
            }))?
        );
    } else {
        for target in targets {
            cli::support::print_open_summary(target, health_path, no_browser)?;
        }

        let databases: Vec<&config::ServiceConfig> = config
            .service
            .iter()
            .filter(|svc| matches!(svc.kind, config::Kind::Database))
            .collect();
        if databases.is_empty() {
            output::event(
                "open",
                LogLevel::Info,
                "DB status: no database services configured",
                Persistence::Persistent,
            );
        } else {
            output::event(
                "open",
                LogLevel::Info,
                "DB status:",
                Persistence::Persistent,
            );
            for svc in databases {
                let status = svc
                    .container_name()
                    .ok()
                    .and_then(|name| docker::inspect_status(&name))
                    .unwrap_or_else(|| "not created".to_owned());
                output::event(
                    "open",
                    LogLevel::Info,
                    &format!("{}: {} ({})", svc.name, status, svc.connection_url()),
                    Persistence::Persistent,
                );
            }
        }
    }

    Ok(())
}
