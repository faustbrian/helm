//! cli support ensure sql service module.
//!
//! Contains cli support ensure sql service logic used by Helm command workflows.

use anyhow::Result;

use crate::config;

/// Ensures sql service exists and is in the required state.
pub(crate) fn ensure_sql_service(service: &config::ServiceConfig, command: &str) -> Result<()> {
    if service.supports_sql_dump() {
        return Ok(());
    }

    anyhow::bail!(
        "'{}' supports SQL database services only. '{}' uses driver '{}'.",
        command,
        service.name,
        format!("{:?}", service.driver).to_lowercase()
    )
}
