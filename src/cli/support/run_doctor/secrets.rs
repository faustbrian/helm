//! cli support run doctor secrets module.
//!
//! Contains cli support run doctor secrets logic used by Helm command workflows.

use anyhow::Result;

use crate::config;
use crate::output::{self, LogLevel, Persistence};

use super::super::find_sensitive_env_values::find_sensitive_env_values;

/// Checks sensitive env values and reports actionable failures.
pub(super) fn check_sensitive_env_values() -> Result<bool> {
    if let Ok(root) = config::project_root() {
        let env_path = root.join(".env");
        if env_path.exists() {
            let sensitive = find_sensitive_env_values(&env_path)?;
            if !sensitive.is_empty() {
                output::event(
                    "doctor",
                    LogLevel::Error,
                    &format!(
                        "Potential real secrets in {}: {}",
                        env_path.display(),
                        sensitive.join(", ")
                    ),
                    Persistence::Persistent,
                );
                output::event(
                    "doctor",
                    LogLevel::Info,
                    &format!("Run: helm env-scrub --env-file {}", env_path.display()),
                    Persistence::Persistent,
                );
                return Ok(true);
            }
        }
    }

    Ok(false)
}
