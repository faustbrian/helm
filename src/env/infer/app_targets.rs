//! Env inference rules for app-type targets.

use std::collections::HashMap;

use crate::config::{Driver, ServiceConfig};

use super::{
    inferred_app_public_url, insert_if_absent, is_app_driver, runtime_host_for_app,
    service_endpoint,
};

/// Applies app-specific inferred env variables.
///
/// Variables are only inserted when absent so explicit user env settings keep
/// precedence.
pub(super) fn apply_app_target_env(vars: &mut HashMap<String, String>, service: &ServiceConfig) {
    if is_app_driver(service, Driver::Frankenphp)
        && let Some(url) = inferred_app_public_url(service)
    {
        insert_if_absent(vars, "APP_URL", url.clone());
        insert_if_absent(vars, "ASSET_URL", url);
    }

    if is_app_driver(service, Driver::Gotenberg) {
        insert_if_absent(vars, "GOTENBERG_BASE_URL", service_endpoint(service));
    }

    if is_app_driver(service, Driver::Mailhog) {
        let smtp_port = service
            .smtp_port
            .unwrap_or(service.port.saturating_add(1000));
        insert_if_absent(vars, "MAIL_MAILER", "smtp".to_owned());
        insert_if_absent(vars, "MAIL_HOST", runtime_host_for_app(service));
        insert_if_absent(vars, "MAIL_PORT", smtp_port.to_string());
        insert_if_absent(vars, "MAIL_ENCRYPTION", "null".to_owned());
        insert_if_absent(vars, "MAIL_USERNAME", String::new());
        insert_if_absent(vars, "MAIL_PASSWORD", String::new());
        insert_if_absent(vars, "MAIL_FROM_ADDRESS", "hello@example.com".to_owned());
        insert_if_absent(vars, "MAIL_FROM_NAME", "Helm".to_owned());
    }
}
