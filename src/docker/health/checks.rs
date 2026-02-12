use anyhow::{Context, Result};
use std::process::Command;

use crate::config::{Driver, ServiceConfig};

use super::http::http_status_code;

pub(super) fn check_service_health(service: &ServiceConfig, container_name: &str) -> Result<bool> {
    match service.driver {
        Driver::Postgres => Command::new("docker")
            .args([
                "exec",
                container_name,
                "pg_isready",
                "-U",
                service.username.as_deref().unwrap_or("postgres"),
            ])
            .output()
            .map(|output| output.status.success())
            .context("postgres health check command failed"),
        Driver::Mysql => Command::new("docker")
            .args([
                "exec",
                container_name,
                "mysqladmin",
                "ping",
                "-u",
                service.username.as_deref().unwrap_or("root"),
                &format!("-p{}", service.password.as_deref().unwrap_or("secret")),
                "--silent",
            ])
            .output()
            .map(|output| output.status.success())
            .context("mysql health check command failed"),
        Driver::Redis | Driver::Valkey => Command::new("docker")
            .args(["exec", container_name, "redis-cli", "PING"])
            .output()
            .map(|output| output.status.success())
            .context("redis health check command failed"),
        Driver::Minio => health_check_http(service, "/minio/health/ready"),
        Driver::Rustfs => health_check_http(service, "/"),
        Driver::Meilisearch => health_check_http(service, "/health"),
        Driver::Typesense => health_check_http(service, "/health"),
        Driver::Frankenphp => health_check_http(service, "/"),
        Driver::Gotenberg => health_check_http(service, "/health"),
        Driver::Mailhog => health_check_http(service, "/"),
    }
}

fn health_check_http(service: &ServiceConfig, path: &str) -> Result<bool> {
    let status_code = http_status_code(&service.host, service.port, path)?;
    Ok(status_code < 500)
}
