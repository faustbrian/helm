use anyhow::{Context, Result};
use std::process::Command;

use crate::config::{Driver, ServiceConfig};
use crate::output::{self, LogLevel, Persistence};

use super::super::setup::ensure_sql_service;
use super::common::{SqlContext, sql_context};

pub(super) fn reset_database(service: &ServiceConfig) -> Result<()> {
    ensure_sql_service(service)?;

    if crate::docker::is_dry_run() {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("[dry-run] Reset database for '{}'", service.name),
            Persistence::Transient,
        );
        return Ok(());
    }

    let ctx = sql_context(service)?;

    output::event(
        &service.name,
        LogLevel::Info,
        &format!("Resetting database '{}'", ctx.db_name),
        Persistence::Persistent,
    );

    let (drop_sql, create_sql) = reset_sql(&ctx);
    run_sql_admin(&ctx, &drop_sql, "drop")?;
    run_sql_admin(&ctx, &create_sql, "create")?;

    output::event(
        &service.name,
        LogLevel::Success,
        &format!("Database '{}' reset", ctx.db_name),
        Persistence::Persistent,
    );
    Ok(())
}

fn reset_sql(ctx: &SqlContext) -> (String, String) {
    match ctx.driver {
        Driver::Postgres => (
            format!("DROP DATABASE IF EXISTS \"{}\"", ctx.db_name),
            format!("CREATE DATABASE \"{}\"", ctx.db_name),
        ),
        Driver::Mysql => (
            format!("DROP DATABASE IF EXISTS `{}`", ctx.db_name),
            format!("CREATE DATABASE `{}`", ctx.db_name),
        ),
        _ => unreachable!("validated SQL drivers in sql_context"),
    }
}

fn run_sql_admin(ctx: &SqlContext, sql: &str, action: &str) -> Result<()> {
    let args = sql_admin_args(ctx, sql);
    let output = Command::new("docker")
        .args(&args)
        .output()
        .with_context(|| format!("Failed to {action} database"))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::bail!("Failed to {action} database: {stderr}");
}

fn sql_admin_args(ctx: &SqlContext, sql: &str) -> Vec<String> {
    match ctx.driver {
        Driver::Postgres => vec![
            "exec".into(),
            ctx.container_name.clone(),
            "psql".into(),
            "-U".into(),
            ctx.username.clone(),
            "-d".into(),
            "postgres".into(),
            "-c".into(),
            sql.to_owned(),
        ],
        Driver::Mysql => vec![
            "exec".into(),
            ctx.container_name.clone(),
            "mysql".into(),
            "-u".into(),
            ctx.username.clone(),
            format!("-p{}", ctx.password),
            "-e".into(),
            sql.to_owned(),
        ],
        _ => unreachable!("validated SQL drivers in sql_context"),
    }
}
