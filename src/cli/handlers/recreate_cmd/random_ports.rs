//! cli handlers recreate cmd random ports module.
//!
//! Contains cli handlers recreate cmd random ports logic used by Helm command workflows.

use anyhow::Result;
use rayon::prelude::*;
use std::collections::HashSet;
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
    if parallel == 0 {
        anyhow::bail!("--parallel must be >= 1");
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
    let mut used_ports: HashSet<u16> = runtime_config.service.iter().map(|svc| svc.port).collect();
    let mut planned = Vec::new();

    for mut runtime in selected {
        runtime.port = cli::support::random_unused_port(&runtime.host, &used_ports)?;
        used_ports.insert(runtime.port);
        if runtime.driver == config::Driver::Mailhog && runtime.smtp_port.is_some() {
            let smtp_port = cli::support::random_unused_port(&runtime.host, &used_ports)?;
            runtime.smtp_port = Some(smtp_port);
            used_ports.insert(smtp_port);
        }
        apply_runtime_binding(&mut runtime_config, &runtime)?;
        planned.push(runtime);
    }

    let runtime_env = env::inferred_app_env(&runtime_config);
    if parallel <= 1 {
        for runtime in &planned {
            recreate_runtime(
                runtime,
                healthy,
                timeout,
                workspace_root,
                &runtime_env,
                quiet,
            )?;
        }
    } else {
        let (app_planned, non_app_planned): (Vec<_>, Vec<_>) = planned
            .iter()
            .partition(|runtime| runtime.kind == config::Kind::App);

        rayon::ThreadPoolBuilder::new()
            .num_threads(parallel)
            .build()?
            .install(|| {
                non_app_planned.par_iter().try_for_each(|runtime| {
                    recreate_runtime(
                        runtime,
                        healthy,
                        timeout,
                        workspace_root,
                        &runtime_env,
                        quiet,
                    )
                })
            })?;

        for runtime in app_planned {
            recreate_runtime(
                runtime,
                healthy,
                timeout,
                workspace_root,
                &runtime_env,
                quiet,
            )?;
        }
    }

    for runtime in &planned {
        if let Some(path) = env_path.as_deref() {
            env::update_env(runtime, path, true)?;
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

#[allow(clippy::too_many_arguments)]
fn recreate_runtime(
    runtime: &config::ServiceConfig,
    healthy: bool,
    timeout: u64,
    workspace_root: &Path,
    runtime_env: &std::collections::HashMap<String, String>,
    quiet: bool,
) -> Result<()> {
    if !quiet {
        output::event(
            &runtime.name,
            LogLevel::Info,
            &format!("Recreating service on random port {}", runtime.port),
            Persistence::Persistent,
        );
    }

    recreate_service(runtime, healthy, timeout, workspace_root, runtime_env)
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

    #[test]
    fn handle_random_ports_recreate_allows_parallel_execution() {
        let mut config = Config {
            schema_version: 1,
            container_prefix: None,
            service: Vec::new(),
            swarm: Vec::new(),
        };
        let config_path_buf: Option<PathBuf> = None;
        let project_root_buf: Option<PathBuf> = None;

        let result = handle_random_ports_recreate(
            &mut config,
            None,
            None,
            false,
            30,
            false,
            false,
            true,
            None,
            None,
            None,
            &config_path_buf,
            &project_root_buf,
            2,
            Path::new("."),
        );

        assert!(result.is_ok());
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
            hook: Vec::new(),
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
