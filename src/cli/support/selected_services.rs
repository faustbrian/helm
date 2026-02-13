//! cli support selected services module.
//!
//! Contains cli support selected services logic used by Helm command workflows.

use anyhow::Result;

use crate::config;

use super::filter_services::filter_services;
use super::matches_filter::matches_filter;

pub(crate) fn selected_services<'a>(
    config: &'a config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    driver: Option<config::Driver>,
) -> Result<Vec<&'a config::ServiceConfig>> {
    if let Some(name) = service {
        let svc = config::find_service(config, name)?;
        if matches_filter(svc, kind, driver) {
            return Ok(vec![svc]);
        }
        return Ok(Vec::new());
    }

    Ok(filter_services(&config.service, kind, driver))
}
