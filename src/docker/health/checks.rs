//! docker health checks module.
//!
//! Contains docker health checks logic used by Helm command workflows.

use anyhow::{Context, Result};
use std::net::TcpStream;
use std::process::Command;

use crate::config::{Driver, ServiceConfig};

use super::http::http_status_code;

/// Checks service health and reports actionable failures.
pub(super) fn check_service_health(service: &ServiceConfig, container_name: &str) -> Result<bool> {
    match service.driver {
        Driver::Mongodb => Command::new("docker")
            .args([
                "exec",
                container_name,
                "mongosh",
                "--eval",
                "db.adminCommand({ ping: 1 })",
            ])
            .output()
            .map(|output| output.status.success())
            .context("mongodb health check command failed"),
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
        Driver::Memcached => health_check_tcp(service),
        Driver::Minio => health_check_http(service, "/minio/health/ready"),
        Driver::Rustfs => health_check_http(service, "/"),
        Driver::Meilisearch => health_check_http(service, "/health"),
        Driver::Typesense => health_check_http(service, "/health"),
        Driver::Frankenphp => health_check_http(service, "/"),
        Driver::Reverb => health_check_http(service, "/"),
        Driver::Horizon => health_check_horizon(container_name),
        Driver::Scheduler => health_check_scheduler(container_name),
        Driver::Dusk => health_check_http(service, "/wd/hub/status"),
        Driver::Gotenberg => health_check_http(service, "/health"),
        Driver::Mailhog => health_check_http(service, "/"),
        Driver::Rabbitmq => health_check_tcp(service),
        Driver::Soketi => health_check_http(service, "/"),
    }
}

fn health_check_tcp(service: &ServiceConfig) -> Result<bool> {
    let address = format!("{}:{}", service.host, service.port);
    Ok(TcpStream::connect(address).is_ok())
}

fn health_check_http(service: &ServiceConfig, path: &str) -> Result<bool> {
    let status_code = http_status_code(&service.host, service.port, path)?;
    Ok(status_code < 500)
}

fn health_check_horizon(container_name: &str) -> Result<bool> {
    Command::new("docker")
        .args(["exec", container_name, "php", "artisan", "horizon:status"])
        .output()
        .map(|output| output.status.success())
        .context("horizon health check command failed")
}

fn health_check_scheduler(container_name: &str) -> Result<bool> {
    Command::new("docker")
        .args(["exec", container_name, "php", "artisan", "schedule:list"])
        .output()
        .map(|output| output.status.success())
        .context("scheduler health check command failed")
}
