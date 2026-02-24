//! Precondition checks for automatic container CA trust.

use anyhow::Result;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

/// Evaluates whether automatic trust installation should run.
///
/// Returns target container/output paths when checks pass; otherwise logs a
/// skip reason and returns `None`.
pub(super) fn evaluate_trust_preconditions(
    target: &ServiceConfig,
) -> Result<Option<(String, String)>> {
    if target.resolved_container_port() != 443 {
        emit_trust_skip(
            target,
            &format!(
                "Skipping container CA trust because container_port is {} (expected 443)",
                target.resolved_container_port()
            ),
        );
        return Ok(None);
    }

    if !cfg!(target_os = "macos") {
        emit_trust_skip(
            target,
            "Skipping container CA trust because automation currently supports macOS only",
        );
        return Ok(None);
    }

    let container_name = target.container_name()?;
    let output_path = format!("/tmp/{container_name}-inner-caddy-root.crt");

    if crate::docker::is_dry_run() {
        let cp_args = vec![
            "cp".to_owned(),
            "<container-ca-path>".to_owned(),
            output_path.clone(),
        ];
        output::event(
            &target.name,
            LogLevel::Info,
            &format!(
                "[dry-run] {}",
                crate::docker::runtime_command_text(&cp_args)
            ),
            Persistence::Transient,
        );
        output::event(
            &target.name,
            LogLevel::Info,
            &format!(
                "[dry-run] sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain {output_path}"
            ),
            Persistence::Transient,
        );
        return Ok(None);
    }

    Ok(Some((container_name, output_path)))
}

fn emit_trust_skip(target: &ServiceConfig, message: &str) {
    output::event(
        &target.name,
        LogLevel::Info,
        message,
        Persistence::Persistent,
    );
}
