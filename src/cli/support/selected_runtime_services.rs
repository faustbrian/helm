//! Owned runtime service selection helpers.

use anyhow::Result;

use crate::config;

use super::selected_services_with_filters;

/// Returns selected services as owned runtime values.
pub(crate) fn selected_runtime_services(
    config: &config::Config,
    service: Option<&str>,
    services: &[String],
    kind: Option<config::Kind>,
    profile: Option<&str>,
) -> Result<Vec<config::ServiceConfig>> {
    Ok(
        selected_services_with_filters(config, service, services, kind, None, profile)?
            .into_iter()
            .cloned()
            .collect(),
    )
}
