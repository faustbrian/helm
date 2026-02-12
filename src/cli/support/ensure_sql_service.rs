use anyhow::Result;

use crate::config;

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
