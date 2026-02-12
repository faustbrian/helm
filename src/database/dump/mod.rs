use anyhow::{Context, Result};
use std::path::Path;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::setup::ensure_sql_service;
use super::sql_admin::run_dump_command;
use io::{is_gzip_path, write_dump_file, write_dump_stdout};

mod io;

pub(crate) fn dump(service: &ServiceConfig, file_path: &Path, gzip: bool) -> Result<()> {
    ensure_sql_service(service)?;

    output::event(
        &service.name,
        LogLevel::Info,
        &format!(
            "Dumping database '{}' to {}",
            service.database.as_deref().unwrap_or("app"),
            file_path.display()
        ),
        Persistence::Persistent,
    );

    if crate::docker::is_dry_run() {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("[dry-run] Dump database '{}'", service.name),
            Persistence::Transient,
        );
        return Ok(());
    }

    let output = run_dump_command(service)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to dump database: {stderr}");
    }

    let use_gzip = gzip || is_gzip_path(file_path);
    write_dump_file(file_path, &output.stdout, use_gzip)?;

    let size = output.stdout.len();
    output::event(
        &service.name,
        LogLevel::Success,
        &format!("Dumped {} ({} bytes raw)", file_path.display(), size),
        Persistence::Persistent,
    );
    Ok(())
}

pub(crate) fn dump_stdout(service: &ServiceConfig, gzip: bool) -> Result<()> {
    ensure_sql_service(service)?;

    if crate::docker::is_dry_run() {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("[dry-run] Stream dump for '{}'", service.name),
            Persistence::Transient,
        );
        return Ok(());
    }

    let output = run_dump_command(service)?;
    if !output.status.success() {
        anyhow::bail!("Failed to dump database");
    }

    write_dump_stdout(&output.stdout, gzip).context("Failed to write dump to stdout")
}
