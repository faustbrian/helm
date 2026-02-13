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
                    "url": database_runtime_url(svc),
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
                    &format!("{}: {} ({})", svc.name, status, database_runtime_url(svc)),
                    Persistence::Persistent,
                );
            }
        }
    }

    Ok(())
}

fn database_runtime_url(service: &config::ServiceConfig) -> String {
    let runtime_binding = service.container_name().ok().and_then(|container_name| {
        docker::inspect_host_port_binding(&container_name, service.resolved_container_port())
    });
    build_database_url_with_binding(service, runtime_binding)
}

fn build_database_url_with_binding(
    service: &config::ServiceConfig,
    runtime_binding: Option<(String, u16)>,
) -> String {
    let mut runtime_service = service.clone();
    if let Some((runtime_host, runtime_port)) = runtime_binding {
        runtime_service.host = normalize_runtime_host(runtime_host, &service.host);
        runtime_service.port = runtime_port;
    }
    runtime_service.connection_url()
}

fn normalize_runtime_host(runtime_host: String, fallback: &str) -> String {
    match runtime_host.as_str() {
        "0.0.0.0" | "::" | "" => fallback.to_owned(),
        _ => runtime_host,
    }
}

#[cfg(test)]
mod tests {
    use super::build_database_url_with_binding;
    use anyhow::Result;

    #[test]
    fn runtime_binding_overrides_configured_database_port() -> Result<()> {
        let mut service = crate::config::preset_preview("mysql")?;
        service.host = "127.0.0.1".to_owned();
        service.port = 33060;
        service.username = Some("laravel".to_owned());
        service.password = Some("laravel".to_owned());
        service.database = Some("laravel".to_owned());

        let url = build_database_url_with_binding(&service, Some(("127.0.0.1".to_owned(), 49123)));
        assert_eq!(url, "mysql://laravel:laravel@127.0.0.1:49123/laravel");
        Ok(())
    }

    #[test]
    fn runtime_binding_falls_back_to_configured_host_for_unspecified_any_host() -> Result<()> {
        let mut service = crate::config::preset_preview("mysql")?;
        service.host = "127.0.0.1".to_owned();
        service.port = 33060;
        service.username = Some("laravel".to_owned());
        service.password = Some("laravel".to_owned());
        service.database = Some("laravel".to_owned());

        let url = build_database_url_with_binding(&service, Some(("0.0.0.0".to_owned(), 49123)));
        assert_eq!(url, "mysql://laravel:laravel@127.0.0.1:49123/laravel");
        Ok(())
    }
}
