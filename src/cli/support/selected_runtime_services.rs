//! Owned runtime service selection helpers.

use anyhow::Result;

use crate::config;

use super::selected_services;

/// Returns selected services as owned runtime values.
pub(crate) fn selected_runtime_services(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
) -> Result<Vec<config::ServiceConfig>> {
    Ok(selected_services(config, service, kind, None)?
        .into_iter()
        .cloned()
        .collect())
}
