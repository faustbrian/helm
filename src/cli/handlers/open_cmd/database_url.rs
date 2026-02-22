//! Runtime database URL helpers for `open` command output.

use crate::{config, docker};

pub(super) fn database_runtime_url(service: &config::ServiceConfig) -> String {
    let runtime_binding = service.container_name().ok().and_then(|container_name| {
        docker::inspect_host_port_binding(&container_name, service.resolved_container_port())
    });
    build_database_url_with_binding(service, runtime_binding)
}

pub(super) fn build_database_url_with_binding(
    service: &config::ServiceConfig,
    runtime_binding: Option<(String, u16)>,
) -> String {
    let mut runtime_service = service.clone();
    if let Some((runtime_host, runtime_port)) = runtime_binding {
        runtime_service.host = normalize_runtime_host(runtime_host, &service.host);
        runtime_service.port = runtime_port;
    }
    runtime_service.connection_url()
}

fn normalize_runtime_host(runtime_host: String, fallback: &str) -> String {
    let normalized = config::normalize_host_for_port_allocation(&runtime_host);
    if config::is_unspecified_port_allocation_host(&normalized) {
        fallback.to_owned()
    } else {
        normalized
    }
}
