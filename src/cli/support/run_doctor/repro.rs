//! cli support run doctor repro module.
//!
//! Contains cli support run doctor repro logic used by Helm command workflows.

use anyhow::Result;
use std::path::Path;

use crate::config;
use crate::output::{self, LogLevel, Persistence};

/// Checks reproducibility and reports actionable failures.
pub(super) fn check_reproducibility(
    config_data: &config::Config,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<bool> {
    let mut has_error = false;

    for service in &config_data.service {
        if service.image.contains("@sha256:") {
            continue;
        }

        has_error = true;
        output::event(
            "doctor",
            LogLevel::Error,
            &format!(
                "Service '{}' uses non-immutable image '{}'",
                service.name, service.image
            ),
            Persistence::Persistent,
        );
    }

    if let Err(err) = config::verify_lockfile_with(config_data, config_path, project_root) {
        has_error = true;
        output::event(
            "doctor",
            LogLevel::Error,
            &format!("Lockfile check failed: {err}"),
            Persistence::Persistent,
        );
    }

    if !has_error {
        output::event(
            "doctor",
            LogLevel::Success,
            "Reproducibility checks passed",
            Persistence::Persistent,
        );
    }

    Ok(has_error)
}
