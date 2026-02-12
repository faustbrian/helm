use std::collections::HashMap;

use crate::config::{Config, Driver, Kind, ServiceConfig};

mod app_targets;
mod service_backends;

/// Builds inferred runtime environment values for an app container from configured services.
///
/// This map is intended for container runtime injection (e.g. `docker run -e ...`) so
/// loopback hosts are rewritten to `host.docker.internal`.
#[must_use]
pub(crate) fn inferred_app_env(config: &Config) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    vars.insert(
        "HELM_SQL_CLIENT_FLAVOR".to_owned(),
        crate::config::preferred_sql_client_flavor(config).to_owned(),
    );

    for service in &config.service {
        if service.kind != Kind::App {
            service_backends::apply_service_env(&mut vars, service);
        }
    }

    for service in &config.service {
        if service.kind == Kind::App {
            app_targets::apply_app_target_env(&mut vars, service);
        }
    }

    vars
}

/// Builds environment values managed by app runtime injection:
/// inferred values plus explicit app target env overrides.
#[must_use]
pub(crate) fn managed_app_env(config: &Config) -> HashMap<String, String> {
    let mut vars = inferred_app_env(config);
    for service in &config.service {
        if service.kind != Kind::App {
            continue;
        }
        if let Some(explicit) = &service.env {
            for (key, value) in explicit {
                insert_if_absent(&mut vars, key, value.clone());
            }
        }
    }
    vars
}

pub(super) fn runtime_host_for_app(service: &ServiceConfig) -> String {
    if service.host == "127.0.0.1" || service.host.eq_ignore_ascii_case("localhost") {
        return "host.docker.internal".to_owned();
    }
    service.host.clone()
}

pub(super) fn insert_if_absent(vars: &mut HashMap<String, String>, key: &str, value: String) {
    if !vars.contains_key(key) {
        vars.insert(key.to_owned(), value);
    }
}

pub(super) fn inferred_app_public_url(service: &ServiceConfig) -> Option<String> {
    if service.localhost_tls {
        return Some(format!("https://localhost:{}", service.port));
    }

    service
        .primary_domain()
        .map(|domain| format!("https://{domain}"))
}

pub(super) fn service_endpoint(service: &ServiceConfig) -> String {
    format!(
        "{}://{}:{}",
        service.scheme(),
        runtime_host_for_app(service),
        service.port
    )
}

pub(super) fn is_app_driver(service: &ServiceConfig, driver: Driver) -> bool {
    service.kind == Kind::App && service.driver == driver
}
