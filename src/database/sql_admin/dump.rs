use anyhow::{Context, Result};
use std::process::Command;

use crate::config::{Driver, ServiceConfig};

use super::super::setup::ensure_sql_service;
use super::common::sql_context;

pub(super) fn run_dump_command(service: &ServiceConfig) -> Result<std::process::Output> {
    ensure_sql_service(service)?;

    let ctx = sql_context(service)?;

    match ctx.driver {
        Driver::Postgres => Command::new("docker")
            .args([
                "exec",
                &ctx.container_name,
                "pg_dump",
                "-U",
                &ctx.username,
                &ctx.db_name,
            ])
            .output()
            .context("Failed to execute pg_dump"),
        Driver::Mysql => Command::new("docker")
            .args([
                "exec",
                &ctx.container_name,
                "mysqldump",
                "-u",
                &ctx.username,
                &format!("-p{}", ctx.password),
                &ctx.db_name,
            ])
            .output()
            .context("Failed to execute mysqldump"),
        _ => unreachable!("validated SQL drivers in sql_context"),
    }
}
