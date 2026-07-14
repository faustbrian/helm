//! cli handlers config cmd module.
//!
//! Contains cli handlers config cmd logic used by Stackctl command workflows.

use anyhow::Result;

use crate::cli::handlers::serialize;
use crate::config;

/// Handles the `config` CLI command.
pub(crate) fn handle_config(config: &config::Config, format: &str) -> Result<()> {
    serialize::print_pretty(config, format)
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::config::{Config, Driver, Kind, ServiceConfig};

    use super::handle_config;

    fn service(name: &str, kind: Kind, driver: Driver) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind,
            driver,
            image: "service:latest".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 3306,
            database: Some("app".to_owned()),
            username: Some("root".to_owned()),
            password: Some("secret".to_owned()),
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: None,
            domain: None,
            domains: None,
            resolved_domain: None,
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
            restart: None,
            localhost_tls: false,
            octane: false,
            octane_workers: None,
            octane_max_requests: None,
            php_extensions: None,
            trust_container_ca: false,
            env_mapping: None,
            javascript: None,
            container_name: Some(format!("{name}-container")),
            resolved_container_name: None,
        }
    }

    fn base_config() -> Config {
        Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: Some("stackctl".to_owned()),
            domain_strategy: None,
            service: vec![service("app", Kind::App, Driver::Frankenphp)],
            swarm: Vec::new(),
        }
    }

    #[test]
    fn handle_config_renders_supported_formats() -> Result<()> {
        let cfg = base_config();
        handle_config(&cfg, "json")?;
        handle_config(&cfg, "toml")?;
        Ok(())
    }

    #[test]
    fn handle_config_rejects_unknown_format() {
        let cfg = base_config();
        assert!(handle_config(&cfg, "yaml").is_err());
    }
}
