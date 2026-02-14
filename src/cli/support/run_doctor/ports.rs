//! cli support run doctor ports module.
//!
//! Contains cli support run doctor ports logic used by Helm command workflows.

use std::collections::HashMap;
use std::net::TcpListener;

use crate::config;
use crate::docker;
use crate::output::{self, LogLevel, Persistence};

/// Checks port conflicts and reports actionable failures.
pub(super) fn check_port_conflicts(config: &config::Config) -> bool {
    let mut has_error = false;
    let mut used_ports: HashMap<(String, u16), &str> = HashMap::new();

    for svc in &config.service {
        let key = (svc.host.clone(), svc.port);
        if let Some(existing) = used_ports.insert(key.clone(), svc.name.as_str()) {
            has_error = true;
            output::event(
                "doctor",
                LogLevel::Error,
                &format!(
                    "Port conflict: {}:{} used by '{}' and '{}'",
                    key.0, key.1, existing, svc.name
                ),
                Persistence::Persistent,
            );
        }

        if service_is_running(svc) {
            continue;
        }

        if !host_port_is_available(&svc.host, svc.port) {
            has_error = true;
            output::event(
                "doctor",
                LogLevel::Error,
                &format!(
                    "Host port unavailable: {}:{} is already in use for '{}'",
                    svc.host, svc.port, svc.name
                ),
                Persistence::Persistent,
            );
        }
    }

    has_error
}

fn service_is_running(service: &config::ServiceConfig) -> bool {
    let Ok(container_name) = service.container_name() else {
        return false;
    };

    docker::inspect_status(&container_name).as_deref() == Some("running")
}

fn host_port_is_available(host: &str, port: u16) -> bool {
    TcpListener::bind((host, port)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::host_port_is_available;
    use std::net::TcpListener;

    #[test]
    fn host_port_availability_detects_busy_port() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind random port");
        let port = listener.local_addr().expect("local addr").port();

        assert!(!host_port_is_available("127.0.0.1", port));
    }

    #[test]
    fn host_port_availability_detects_free_port() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind random port");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);

        assert!(host_port_is_available("127.0.0.1", port));
    }
}
