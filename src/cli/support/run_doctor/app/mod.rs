//! cli support run doctor app module.
//!
//! Contains cli support run doctor app logic used by Helm command workflows.

use crate::config;

use super::super::app_services::app_services;
use checks::{
    check_domain_resolution, check_http_reachability, check_octane_runtime, check_php_extensions,
};

mod checks;

/// Checks app services and reports actionable failures.
pub(super) fn check_app_services(
    config: &config::Config,
    fix: bool,
    allow_stopped_runtime_checks: bool,
) -> bool {
    let mut has_error = false;

    for target in app_services(config) {
        has_error |= check_domain_resolution(target, fix);
        has_error |= check_php_extensions(target);

        if target.octane {
            has_error |= check_octane_runtime(target, allow_stopped_runtime_checks);
        }
    }

    has_error
}

/// Checks app URL and health endpoint reachability.
pub(super) fn check_reachability(config: &config::Config) -> bool {
    let mut has_error = false;

    for target in app_services(config) {
        has_error |= check_http_reachability(target);
    }

    has_error
}
