//! Public serve API surface.
//!
//! Re-exports the high-level `run/down/public_url` workflow and selected helpers
//! used by CLI handlers.

#![allow(clippy::print_stdout)] // CLI command prints user-facing status
#![allow(clippy::fn_params_excessive_bools)] // CLI flags are modeled directly as bools
#![allow(clippy::format_push_string)] // String building prioritizes readability in generators

use anyhow::Result;

use crate::config::ServiceConfig;

mod caddy;
mod container;
mod exec;
mod extensions;
mod health;
mod hosts;
mod image_build;
mod images;
mod orchestrate;
mod sql_client_flavor;
mod state;
#[cfg(test)]
mod tests;
mod trust;

pub use caddy::{caddy_access_log_path, trust_local_caddy_ca};
pub(crate) use exec::{exec_artisan, exec_or_run_command, runtime_cmdline};
pub(crate) use hosts::{domain_resolves_to_loopback, ensure_hosts_entry_for_domain};
pub use state::PhpExtensionCheck;

use container::resolve_volume_mapping;
use images::{
    mailhog_smtp_port, normalize_php_extensions, resolve_runtime_image,
    should_inject_frankenphp_server_name,
};

use caddy::{
    configure_caddy, domain_for_service as service_domain, domains_for_service as service_domains,
    remove_caddy_route, resolve_caddy_ports, served_url,
};
use container::{ensure_container_running, remove_container};
use hosts::ensure_hosts_entry;
pub(super) use state::{CaddyPorts, CaddyState, DerivedImageLock};
use trust::trust_inner_container_ca;

pub use orchestrate::{down, public_url, run};

/// Waits until app HTTP endpoint responds with a 2xx status code.
///
/// # Errors
///
/// Returns an error if timeout is reached or curl execution fails repeatedly.
pub fn wait_until_http_healthy(
    target: &ServiceConfig,
    timeout_secs: u64,
    interval_secs: u64,
    health_path: Option<&str>,
) -> Result<()> {
    health::wait_until_http_healthy(target, timeout_secs, interval_secs, health_path)
}

/// Verifies `php_extensions` declared for a serve target are present in runtime image.
///
/// # Errors
///
/// Returns an error if image build or verification command execution fails.
pub fn verify_php_extensions(target: &ServiceConfig) -> Result<Option<PhpExtensionCheck>> {
    extensions::verify_php_extensions(target)
}
