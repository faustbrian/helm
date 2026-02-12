use crate::config;

use super::super::app_services::app_services;
use checks::{check_domain_resolution, check_octane_runtime, check_php_extensions};

mod checks;

pub(super) fn check_app_services(config: &config::Config, fix: bool) -> bool {
    let mut has_error = false;

    for target in app_services(config) {
        has_error |= check_domain_resolution(target, fix);
        has_error |= check_php_extensions(target);

        if target.octane {
            has_error |= check_octane_runtime(target);
        }
    }

    has_error
}
