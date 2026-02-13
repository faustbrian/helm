//! cli support run doctor module.
//!
//! Contains cli support run doctor logic used by Helm command workflows.

use anyhow::Result;
use std::path::Path;

use crate::config;
use crate::output::{self, LogLevel, Persistence};

mod app;
mod docker;
mod ports;
mod repro;
mod secrets;

/// Runs the doctor support workflow.
pub(crate) fn run_doctor(
    config: &config::Config,
    fix: bool,
    repro: bool,
    reachability: bool,
    allow_stopped_app_runtime_checks: bool,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<()> {
    let mut has_error = false;

    has_error |= docker::check_docker_availability();
    has_error |= ports::check_port_conflicts(config);
    has_error |= app::check_app_services(config, fix, allow_stopped_app_runtime_checks);
    if reachability {
        has_error |= app::check_reachability(config);
    }

    if fix {
        if let Err(err) = crate::serve::trust_local_caddy_ca() {
            output::event(
                "doctor",
                LogLevel::Error,
                &format!("Caddy trust failed: {err}"),
                Persistence::Persistent,
            );
            has_error = true;
        } else {
            output::event(
                "doctor",
                LogLevel::Success,
                "Caddy local CA trust attempted",
                Persistence::Persistent,
            );
        }
    }

    has_error |= secrets::check_sensitive_env_values()?;
    if repro {
        has_error |= repro::check_reproducibility(config, config_path, project_root)?;
    }

    if has_error {
        anyhow::bail!("doctor found issues")
    }

    output::event(
        "doctor",
        LogLevel::Success,
        "Doctor checks passed",
        Persistence::Persistent,
    );
    Ok(())
}
