//! database sql admin create module.
//!
//! Contains database sql admin create logic used by Helm command workflows.

use anyhow::{Context, Result};
use std::process::Command;

use crate::config::{Driver, ServiceConfig};
use crate::output::{self, LogLevel, Persistence};

use super::super::setup::ensure_sql_service;
use super::common::sql_context;

/// Creates database for downstream execution.
pub(super) fn create_database(service: &ServiceConfig) -> Result<()> {
    ensure_sql_service(service)?;

    if crate::docker::is_dry_run() {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!(
                "[dry-run] Create database '{}' if missing",
                service.database.as_deref().unwrap_or("app")
            ),
            Persistence::Transient,
        );
        return Ok(());
    }

    let ctx = sql_context(service)?;

    let output = match ctx.driver {
        Driver::Postgres => {
            let sql = format!(
                "SELECT 'CREATE DATABASE {}' WHERE NOT EXISTS \
                 (SELECT FROM pg_database WHERE datname = '{}')\\gexec",
                ctx.db_name, ctx.db_name
            );

            Command::new("docker")
                .args([
                    "exec",
                    &ctx.container_name,
                    "psql",
                    "-U",
                    &ctx.username,
                    "-c",
                    &sql,
                ])
                .output()
                .context("Failed to execute postgres CREATE DATABASE command")?
        }
        Driver::Mysql => {
            let sql = format!("CREATE DATABASE IF NOT EXISTS `{}`", ctx.db_name);

            Command::new("docker")
                .args([
                    "exec",
                    &ctx.container_name,
                    "mysql",
                    "-u",
                    &ctx.username,
                    &format!("-p{}", ctx.password),
                    "-e",
                    &sql,
                ])
                .output()
                .context("Failed to execute mysql CREATE DATABASE command")?
        }
        _ => unreachable!("validated SQL drivers in sql_context"),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to create database: {stderr}");
    }

    Ok(())
}
