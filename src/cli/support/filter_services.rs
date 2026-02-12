use crate::config;

use super::matches_filter::matches_filter;

pub(crate) fn filter_services(
    services: &[config::ServiceConfig],
    kind: Option<config::Kind>,
    driver: Option<config::Driver>,
) -> Vec<&config::ServiceConfig> {
    services
        .iter()
        .filter(|svc| matches_filter(svc, kind, driver))
        .collect()
}
