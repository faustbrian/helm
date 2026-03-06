//! Share target URL resolution helpers.

use crate::config::{
    ServiceConfig, is_unspecified_port_allocation_host, normalize_host_for_port_allocation,
};
use crate::docker;
use std::process::Command;

pub(super) fn local_target_url(target: &ServiceConfig) -> String {
    let (host, port) = runtime_host_port(target);
    let scheme = resolved_local_scheme(target, &host, port);
    format!("{scheme}://{host}:{port}")
}

fn local_bind_host(host: &str) -> String {
    if is_unspecified_port_allocation_host(host) {
        return "127.0.0.1".to_owned();
    }

    normalize_host_for_port_allocation(host)
}

fn runtime_host_port(target: &ServiceConfig) -> (String, u16) {
    let Some(container_name) = target.container_name().ok() else {
        return (local_bind_host(&target.host), target.port);
    };

    let Some((runtime_host, runtime_port)) =
        docker::inspect_host_port_binding(&container_name, target.resolved_container_port())
    else {
        return (local_bind_host(&target.host), target.port);
    };

    let host = normalize_host_for_port_allocation(&runtime_host);
    if is_unspecified_port_allocation_host(&host) {
        (local_bind_host(&target.host), runtime_port)
    } else {
        (host, runtime_port)
    }
}

fn resolved_local_scheme(target: &ServiceConfig, host: &str, port: u16) -> &'static str {
    if target.localhost_tls {
        return "https";
    }
    if target.scheme.as_deref() == Some("https") {
        return "https";
    }
    if target.scheme.as_deref() == Some("http") {
        return "http";
    }

    if probe_url_reachable(&format!("https://{host}:{port}")) {
        return "https";
    }
    if probe_url_reachable(&format!("http://{host}:{port}")) {
        return "http";
    }

    "http"
}

fn probe_url_reachable(url: &str) -> bool {
    Command::new("curl")
        .args([
            "-k",
            "-s",
            "-o",
            "/dev/null",
            "--connect-timeout",
            "1",
            "--max-time",
            "2",
            url,
        ])
        .status()
        .is_ok_and(|status| status.success())
}
