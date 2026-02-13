//! env files write module.
//!
//! Contains env files write logic used by Helm command workflows.

use anyhow::Result;
use std::collections::BTreeMap;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::parse::env_key;
use super::{read_env_lines, write_env_lines};
use crate::output::{self, LogLevel, Persistence};

/// Writes explicit environment values into an env file.
///
/// Existing keys are replaced; with `create_missing=true`, absent keys are appended.
///
/// # Errors
///
/// Returns an error if the env file cannot be read or written.
pub(crate) fn write_env_values(
    env_path: &Path,
    values: &HashMap<String, String>,
    create_missing: bool,
) -> Result<()> {
    write_env_values_with_purge(env_path, values, create_missing, &HashSet::new(), false)
}

/// Writes explicit environment values into an env file, optionally purging
/// stale managed keys that are no longer present in `values`.
///
/// Existing keys are replaced; with `create_missing=true`, absent keys are
/// appended.
///
/// # Errors
///
/// Returns an error if the env file cannot be read or written.
pub(crate) fn write_env_values_with_purge(
    env_path: &Path,
    values: &HashMap<String, String>,
    create_missing: bool,
    managed_keys: &HashSet<String>,
    purge_missing_managed: bool,
) -> Result<()> {
    if !env_path.exists() {
        return Ok(());
    }

    let mut lines = read_env_lines(env_path)?;
    let mut updated: Vec<String> = Vec::new();

    for line in &mut lines {
        let Some(key) = env_key(line).map(ToOwned::to_owned) else {
            continue;
        };

        if let Some(value) = values.get(&key) {
            *line = format!("{key}=\"{value}\"");
            updated.push(key);
        }
    }

    if create_missing {
        for (key, value) in values {
            if !updated.iter().any(|existing| existing == key) {
                lines.push(format!("{key}=\"{value}\""));
                updated.push(key.clone());
            }
        }
    }

    if purge_missing_managed {
        lines.retain(|line| {
            let Some(key) = env_key(line) else {
                return true;
            };
            !managed_keys.contains(key) || values.contains_key(key)
        });
    }

    write_env_lines(env_path, lines)?;

    if !updated.is_empty() {
        output::event(
            "env",
            LogLevel::Success,
            &format!(
                "Updated inferred app env values in {}: {}",
                env_path.display(),
                updated.join(", ")
            ),
            Persistence::Persistent,
        );
    }

    Ok(())
}

/// Writes env values full to persisted or external state.
pub(crate) fn write_env_values_full(
    env_path: &Path,
    values: &HashMap<String, String>,
) -> Result<()> {
    let ordered: BTreeMap<&String, &String> = values.iter().collect();
    let lines = ordered
        .into_iter()
        .map(|(key, value)| format!("{key}=\"{value}\""))
        .collect();
    write_env_lines(env_path, lines)
}
