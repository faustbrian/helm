//! Shared non-app recreate helper for lifecycle handlers.

use anyhow::Result;

use crate::{config, docker};

/// Recreates a non-app service, optionally starts it, and optionally waits.
pub(crate) fn recreate_non_app_service(
    service: &config::ServiceConfig,
    start_after_recreate: bool,
    start_pull_policy: docker::PullPolicy,
    wait_healthy: bool,
    health_timeout_secs: u64,
) -> Result<()> {
    docker::recreate(service)?;
    if start_after_recreate {
        docker::up(service, start_pull_policy, false)?;
    }
    if wait_healthy {
        docker::wait_until_healthy(service, health_timeout_secs, 2, None)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::config::{Driver, Kind, ServiceConfig};

    use super::recreate_non_app_service;

    fn service(name: &str) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind: Kind::Database,
            driver: Driver::Postgres,
            image: "postgres:16".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 5432,
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
            container_name: Some(name.to_owned()),
            resolved_container_name: Some(name.to_owned()),
        }
    }

    #[test]
    fn recreate_non_app_service_succeeds_without_start_or_wait() {
        crate::docker::with_dry_run_lock(|| {
            recreate_non_app_service(
                &service("db"),
                false,
                crate::docker::PullPolicy::Missing,
                false,
                30,
            )
            .expect("no-op path");
        });
    }

    #[test]
    fn recreate_non_app_service_starts_and_waits_when_requested() {
        crate::docker::with_dry_run_lock(|| {
            recreate_non_app_service(
                &service("db"),
                true,
                crate::docker::PullPolicy::Missing,
                true,
                30,
            )
            .expect("start and wait path");
        });
    }

    #[test]
    fn recreate_non_app_service_respects_start_pull_policy() {
        let _ = crate::docker::PullPolicy::Always;
        crate::docker::with_dry_run_lock(|| {
            recreate_non_app_service(
                &service("db-cache"),
                true,
                crate::docker::PullPolicy::Always,
                false,
                1,
            )
            .expect("always policy path");
        });
    }
}
