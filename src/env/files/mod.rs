//! env files module.
//!
//! Contains env files logic used by Helm command workflows.

use crate::output::{self, LogLevel, Persistence};
use anyhow::{Context, Result};
use std::path::Path;

mod parse;
mod update;
mod write;

pub(crate) use update::update_env;
pub(crate) use write::{write_env_values, write_env_values_full, write_env_values_with_purge};

/// Reads env lines from persisted or external state.
fn read_env_lines(env_path: &Path) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(env_path)
        .with_context(|| format!("failed to read {}", env_path.display()))?;
    Ok(content.lines().map(str::to_owned).collect())
}

/// Writes env lines to persisted or external state.
fn write_env_lines(env_path: &Path, lines: Vec<String>) -> Result<()> {
    let mut output = lines.join("\n");
    output.push('\n');

    if crate::docker::is_dry_run() {
        output::event(
            "env",
            LogLevel::Info,
            &format!("[dry-run] Write {}", env_path.display()),
            Persistence::Transient,
        );
    } else {
        std::fs::write(env_path, output)
            .with_context(|| format!("failed to write {}", env_path.display()))?;
    }

    Ok(())
}
