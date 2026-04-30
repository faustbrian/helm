//! Test runtime startup helpers for artisan flows.

use anyhow::Result;

use crate::{cli, config, docker};

const TEST_HEALTH_TIMEOUT_SECS: u64 = 60;

pub(super) fn resolve_testing_startup_services<'a>(
    config: &'a config::Config,
    selected_service: Option<&'a str>,
) -> Result<Vec<&'a config::ServiceConfig>> {
    cli::support::resolve_up_services(config, selected_service, None, None)
}

pub(super) fn run_testing_startup_services<F>(
    startup_services: &[&config::ServiceConfig],
    start_context: &cli::support::ServiceStartContext<'_>,
    reset_service_runtime: F,
) -> Result<()>
where
    F: Fn(&config::ServiceConfig) -> Result<()>,
{
    for svc in startup_services {
        let wait_healthy = svc.kind != config::Kind::App;
        reset_service_runtime(svc)?;
        cli::support::start_service(
            svc,
            start_context,
            false,
            docker::PullPolicy::Missing,
            wait_healthy,
            TEST_HEALTH_TIMEOUT_SECS,
            true,
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::resolve_testing_startup_services;
    use crate::config::{Config, Driver, Kind, ProjectType, ServiceConfig};

    #[test]
    fn resolve_testing_startup_services_scopes_to_selected_app_dependencies() {
        let config = Config {
            schema_version: 1,
            project_type: ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![
                service("db", Kind::Database, Driver::Postgres, None),
                service("app", Kind::App, Driver::Frankenphp, Some(vec!["db"])),
                service("profile-app", Kind::App, Driver::Frankenphp, Some(vec!["db"])),
            ],
            swarm: Vec::new(),
        };

        let scoped =
            resolve_testing_startup_services(&config, Some("app")).expect("resolve services");
        let names: Vec<&str> = scoped.iter().map(|service| service.name.as_str()).collect();

        assert_eq!(names, vec!["db", "app"]);
    }

    fn service(
        name: &str,
        kind: Kind,
        driver: Driver,
        depends_on: Option<Vec<&str>>,
    ) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind,
            driver,
            image: "test-image".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 8080,
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
            resolved_domain: None,
            container_port: None,
            smtp_port: None,
            volumes: None,
            env: None,
            command: None,
            depends_on: depends_on.map(|items| items.into_iter().map(str::to_owned).collect()),
            seed_file: None,
            hook: Vec::new(),
            health_path: None,
            health_statuses: None,
            localhost_tls: false,
            octane: false,
            octane_workers: None,
            octane_max_requests: None,
            php_extensions: None,
            trust_container_ca: false,
            env_mapping: None,
            javascript: None,
            container_name: None,
            resolved_container_name: None,
        }
    }
}
