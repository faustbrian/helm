//! cli handlers artisan cmd runtime module.
//!
//! Contains cli handlers artisan cmd runtime logic used by Helm command workflows.

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::{cli, config, docker, serve};

/// Ensures test services running exists and is in the required state.
pub(super) fn ensure_test_services_running(
    config: &mut config::Config,
    workspace_root: &Path,
    inferred_env: &HashMap<String, String>,
) -> Result<()> {
    let remapped_services = assign_testing_runtime_ports(config)?;
    let startup_services = cli::support::resolve_up_services(config, None, None, None)?;
    for svc in startup_services {
        let recreate = remapped_services.contains(&svc.name);
        if svc.kind == config::Kind::App {
            serve::run(
                svc,
                recreate,
                svc.trust_container_ca,
                true,
                workspace_root,
                inferred_env,
                true,
            )?;
        } else {
            docker::up(
                svc,
                docker::UpOptions {
                    pull: docker::PullPolicy::Missing,
                    recreate,
                },
            )?;
        }
    }
    Ok(())
}

fn assign_testing_runtime_ports(config: &mut config::Config) -> Result<HashSet<String>> {
    let mut remapped = HashSet::new();
    let mut used_ports: HashSet<u16> = config.service.iter().map(|service| service.port).collect();

    for service in &config.service {
        if let Some(smtp_port) = service.smtp_port {
            used_ports.insert(smtp_port);
        }
    }

    for service in &mut config.service {
        let remapped_port = cli::support::random_unused_port(&service.host, &used_ports)?;
        if remapped_port != service.port {
            service.port = remapped_port;
            remapped.insert(service.name.clone());
        }
        used_ports.insert(service.port);

        if let Some(smtp_port) = service.smtp_port {
            let remapped_smtp = cli::support::random_unused_port(&service.host, &used_ports)?;
            if remapped_smtp != smtp_port {
                remapped.insert(service.name.clone());
            }
            service.smtp_port = Some(remapped_smtp);
            used_ports.insert(remapped_smtp);
        }
    }

    Ok(remapped)
}
#[cfg(test)]
mod tests {
    use super::assign_testing_runtime_ports;
    use crate::cli;
    use crate::config::{Config, Driver, Kind, ServiceConfig};

    #[test]
    fn assigns_random_service_port_for_testing_runtime() {
        let configured_port = cli::support::random_free_port("127.0.0.1").expect("config port");
        let mut config = Config {
            schema_version: 1,
            container_prefix: None,
            service: vec![service("valkey", configured_port, None)],
            swarm: Vec::new(),
        };

        let remapped = assign_testing_runtime_ports(&mut config).expect("assign ports");
        let remapped_service = config.service.first().expect("service exists");

        assert!(remapped.contains("valkey"));
        assert_ne!(remapped_service.port, configured_port);
    }

    #[test]
    fn assigns_random_smtp_port_for_testing_runtime() {
        let app_port = cli::support::random_free_port("127.0.0.1").expect("app test port");
        let smtp_port = cli::support::random_free_port("127.0.0.1").expect("smtp test port");
        let mut config = Config {
            schema_version: 1,
            container_prefix: None,
            service: vec![service("mailhog", app_port, Some(smtp_port))],
            swarm: Vec::new(),
        };

        let remapped = assign_testing_runtime_ports(&mut config).expect("assign ports");
        let remapped_service = config.service.first().expect("service exists");

        assert!(remapped.contains("mailhog"));
        assert_ne!(remapped_service.smtp_port, Some(smtp_port));
    }

    fn service(name: &str, port: u16, smtp_port: Option<u16>) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind: Kind::Cache,
            driver: Driver::Valkey,
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
            smtp_port,
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
