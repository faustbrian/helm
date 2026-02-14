//! Container stop/remove steps for serve lifecycle teardown.

use anyhow::Result;
use std::process::Command;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

/// Stops then removes the target serve container.
pub(super) fn remove_container(target: &ServiceConfig) -> Result<()> {
    let container_name = target.container_name()?;

    if crate::docker::is_dry_run() {
        output::event(
            &target.name,
            LogLevel::Info,
            &format!("[dry-run] docker stop {container_name}"),
            Persistence::Transient,
        );
        output::event(
            &target.name,
            LogLevel::Info,
            &format!("[dry-run] docker rm {container_name}"),
            Persistence::Transient,
        );
        return Ok(());
    }

    let stop_result = run_docker_step(["stop", &container_name], "stop", &container_name)?;
    let rm_result = run_docker_step(["rm", &container_name], "remove", &container_name)?;

    if matches!(stop_result, StepOutcome::Missing) && matches!(rm_result, StepOutcome::Missing) {
        output::event(
            &target.name,
            LogLevel::Info,
            &format!("Serve container {container_name} is already absent"),
            Persistence::Persistent,
        );
        return Ok(());
    }

    output::event(
        &target.name,
        LogLevel::Success,
        &format!("Stopped and removed serve container {container_name}"),
        Persistence::Persistent,
    );
    Ok(())
}

#[derive(Clone, Copy)]
enum StepOutcome {
    Performed,
    Missing,
}

fn run_docker_step(args: [&str; 2], action: &str, container_name: &str) -> Result<StepOutcome> {
    let output = Command::new("docker")
        .args(args)
        .output()
        .map_err(|error| anyhow::anyhow!("failed to execute docker {action}: {error}"))?;

    if output.status.success() {
        return Ok(StepOutcome::Performed);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.to_ascii_lowercase().contains("no such container") {
        return Ok(StepOutcome::Missing);
    }

    anyhow::bail!("failed to {action} serve container '{container_name}': {stderr}");
}

#[cfg(test)]
mod tests {
    use super::remove_container;
    use crate::config::{Driver, Kind, ServiceConfig};

    #[test]
    fn remove_container_propagates_docker_command_failures() {
        let mut service = service("app");
        service.container_name = Some("bad\0name".to_owned());

        let error = remove_container(&service).expect_err("expected docker stop/rm failure");
        assert!(error.to_string().contains("failed"));
    }

    fn service(name: &str) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
            image: "dunglas/frankenphp:php8.5".to_owned(),
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
            container_port: Some(80),
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
            resolved_container_name: None,
        }
    }
}
