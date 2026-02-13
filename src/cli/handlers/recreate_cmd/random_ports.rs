//! cli handlers recreate cmd random ports module.
//!
//! Contains cli handlers recreate cmd random ports logic used by Helm command workflows.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::output::{self, LogLevel, Persistence};
use crate::{cli, config, env};

use super::recreate_service;

/// Handles the `random ports recreate` CLI command.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_random_ports_recreate(
    config: &mut config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    healthy: bool,
    timeout: u64,
    save_ports: bool,
    env_output: bool,
    quiet: bool,
    runtime_env: Option<&str>,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
    config_path_buf: &Option<PathBuf>,
    project_root_buf: &Option<PathBuf>,
    parallel: usize,
    workspace_root: &Path,
) -> Result<()> {
    if parallel > 1 {
        anyhow::bail!("--publish-all cannot be combined with --parallel > 1");
    }

    let env_path = if env_output {
        Some(cli::support::default_env_path(
            config_path_buf,
            project_root_buf,
            &None,
            runtime_env,
        )?)
    } else {
        None
    };
    let selected: Vec<config::ServiceConfig> =
        cli::support::selected_services(config, service, kind, None)?
            .into_iter()
            .cloned()
            .collect();
    let mut runtime_config = config.clone();

    for mut runtime in selected {
        runtime.port = cli::support::random_free_port(&runtime.host)?;
        if runtime.driver == config::Driver::Mailhog && runtime.smtp_port.is_some() {
            runtime.smtp_port = Some(random_distinct_port(&runtime.host, runtime.port)?);
        }
        if !quiet {
            output::event(
                &runtime.name,
                LogLevel::Info,
                &format!("Recreating service on random port {}", runtime.port),
                Persistence::Persistent,
            );
        }

        apply_runtime_binding(&mut runtime_config, &runtime)?;
        let runtime_env = env::inferred_app_env(&runtime_config);

        recreate_service(&runtime, healthy, timeout, workspace_root, &runtime_env)?;
        if let Some(path) = env_path.as_deref() {
            env::update_env(&runtime, path, true)?;
        }

        if save_ports {
            config::update_service_port(config, &runtime.name, runtime.port)?;
        }
    }

    if save_ports {
        let path = config::save_config_with(config, config_path, project_root)?;
        if !quiet {
            output::event(
                "recreate",
                LogLevel::Success,
                &format!("Persisted random ports to {}", path.display()),
                Persistence::Persistent,
            );
        }
    }

    Ok(())
}

fn random_distinct_port(host: &str, avoid: u16) -> Result<u16> {
    for _ in 0..20 {
        let candidate = cli::support::random_free_port(host)?;
        if candidate != avoid {
            return Ok(candidate);
        }
    }
    anyhow::bail!("failed to allocate random port distinct from {}", avoid);
}

fn apply_runtime_binding(
    runtime_config: &mut config::Config,
    runtime_service: &config::ServiceConfig,
) -> Result<()> {
    let Some(existing) = runtime_config
        .service
        .iter_mut()
        .find(|service| service.name == runtime_service.name)
    else {
        anyhow::bail!(
            "service '{}' not found while applying runtime binding",
            runtime_service.name
        );
    };

    existing.host = runtime_service.host.clone();
    existing.port = runtime_service.port;
    existing.smtp_port = runtime_service.smtp_port;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Driver, Kind, ServiceConfig};

    #[test]
    fn random_distinct_port_avoids_requested_port() {
        let avoid = cli::support::random_free_port("127.0.0.1").expect("allocate avoid port");
        let candidate = random_distinct_port("127.0.0.1", avoid).expect("allocate candidate port");
        assert_ne!(candidate, avoid);
    }

    #[test]
    fn apply_runtime_binding_updates_matching_service_port() {
        let mut runtime_config = Config {
            schema_version: 1,
            container_prefix: None,
            service: vec![service("valkey", 6379), service("app", 8080)],
            swarm: Vec::new(),
        };
        let runtime_service = service("valkey", 50031);

        apply_runtime_binding(&mut runtime_config, &runtime_service).expect("apply runtime");

        let updated = runtime_config
            .service
            .iter()
            .find(|service| service.name == "valkey")
            .expect("service exists");
        assert_eq!(updated.port, 50031);
    }

    fn service(name: &str, port: u16) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind: if name == "app" {
                Kind::App
            } else {
                Kind::Cache
            },
            driver: if name == "app" {
                Driver::Frankenphp
            } else {
                Driver::Valkey
            },
            image: "test-image".to_owned(),
            host: "127.0.0.1".to_owned(),
            port,
            database: None,
            username: None,
            password: None,
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: None,
            domain: None,
            domains: None,
            container_port: None,
            smtp_port: None,
            volumes: None,
            env: None,
            command: None,
            depends_on: None,
            seed_file: None,
            health_path: None,
            health_statuses: None,
            localhost_tls: false,
            octane: false,
            php_extensions: None,
            trust_container_ca: false,
            env_mapping: None,
            container_name: None,
            resolved_container_name: None,
        }
    }
}
