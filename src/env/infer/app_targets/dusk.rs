//! Laravel Dusk app-target env inference.

use std::collections::HashMap;

use crate::config::ServiceConfig;

use super::super::{insert_if_absent, service_endpoint};

/// Applies inferred Dusk WebDriver endpoint variables.
pub(super) fn apply(vars: &mut HashMap<String, String>, service: &ServiceConfig) {
    insert_if_absent(
        vars,
        "DUSK_DRIVER_URL",
        format!("{}/wd/hub", service_endpoint(service)),
    );
}
