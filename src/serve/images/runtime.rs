//! Runtime image option helpers independent of build/cache logic.

use std::collections::HashMap;

use crate::config::{Driver, ServiceConfig};

/// Normalizes php extensions into a canonical form.
pub(super) fn normalize_php_extensions(extensions: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    for ext in extensions {
        let value = if ext.eq_ignore_ascii_case("sqlite") {
            "sqlite3".to_owned()
        } else {
            ext.clone()
        };

        if !normalized.iter().any(|existing| existing == &value) {
            normalized.push(value);
        }
    }
    normalized
}

/// Returns whether FrankenPHP `SERVER_NAME` should be injected automatically.
pub(super) fn should_inject_frankenphp_server_name(
    target: &ServiceConfig,
    injected_env: &HashMap<String, String>,
) -> bool {
    if target.driver != Driver::Frankenphp || target.resolved_container_port() != 80 {
        return false;
    }

    if injected_env.contains_key("SERVER_NAME") {
        return false;
    }

    target
        .env
        .as_ref()
        .is_none_or(|values| !values.contains_key("SERVER_NAME"))
}

/// Returns Mailhog SMTP publish port, defaulting to `http_port + 1000`.
pub(super) fn mailhog_smtp_port(target: &ServiceConfig) -> Option<u16> {
    if target.driver != Driver::Mailhog {
        return None;
    }
    Some(
        target
            .smtp_port
            .unwrap_or_else(|| target.port.saturating_add(1000)),
    )
}
