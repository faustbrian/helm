//! database setup module.
//!
//! Contains database setup logic used by Helm command workflows.

use anyhow::{Context, Result};

use crate::config::ServiceConfig;
use crate::docker::{PullPolicy, UpOptions};
use crate::output::{self, LogLevel, Persistence};

use super::sql_admin::create_database;

/// Ensures sql service exists and is in the required state.
pub(super) fn ensure_sql_service(service: &ServiceConfig) -> Result<()> {
    if service.is_database() {
        return Ok(());
    }

    anyhow::bail!("service '{}' is not a SQL database", service.name)
}

/// Ensures SQL service supports dump/restore operations.
pub(super) fn ensure_sql_dump_service(service: &ServiceConfig) -> Result<()> {
    if service.supports_sql_dump() {
        return Ok(());
    }

    anyhow::bail!(
        "service '{}' does not support SQL dump/restore operations",
        service.name
    )
}

pub(crate) fn setup(service: &ServiceConfig, timeout: u64) -> Result<()> {
    crate::docker::up(
        service,
        UpOptions {
            pull: PullPolicy::Missing,
            recreate: false,
        },
    )
    .context("Failed to start service container")?;

    crate::docker::wait_until_healthy(service, timeout, 2, None)?;

    if service.supports_sql_dump() {
        create_database(service)?;
    }

    output::event(
        &service.name,
        LogLevel::Success,
        "Service is ready",
        Persistence::Persistent,
    );
    Ok(())
}
