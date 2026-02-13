//! cli support matches filter module.
//!
//! Contains cli support matches filter logic used by Helm command workflows.

use crate::config;

pub(crate) fn matches_filter(
    svc: &config::ServiceConfig,
    kind: Option<config::Kind>,
    driver: Option<config::Driver>,
) -> bool {
    let kind_ok = kind.is_none_or(|k| k == svc.kind);
    let driver_ok = driver.is_none_or(|d| d == svc.driver);
    kind_ok && driver_ok
}
