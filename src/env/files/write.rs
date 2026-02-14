//! env files write module.
//!
//! Contains env files write logic used by Helm command workflows.

use anyhow::Result;
use std::collections::BTreeMap;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::parse::env_key;
use super::quote_env_value;
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
        anyhow::bail!("env file not found: {}", env_path.display());
    }

    let mut lines = read_env_lines(env_path)?;
    let mut updated: Vec<String> = Vec::new();

    for line in &mut lines {
        let Some(key) = env_key(line).map(ToOwned::to_owned) else {
            continue;
        };

        if let Some(value) = values.get(&key) {
            *line = format!("{key}={}", quote_env_value(value));
            updated.push(key);
        }
    }

    if create_missing {
        for (key, value) in values {
            if !updated.iter().any(|existing| existing == key) {
                lines.push(format!("{key}={}", quote_env_value(value)));
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
        .map(|(key, value)| format!("{key}={}", quote_env_value(value)))
        .collect();
    write_env_lines(env_path, lines)
}

#[cfg(test)]
mod tests {
    use super::{write_env_values_full, write_env_values_with_purge};
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;

    #[test]
    fn write_env_values_with_purge_errors_when_env_file_is_missing() {
        let env_path = unique_path("missing");
        let values = HashMap::from([(String::from("APP_URL"), String::from("http://localhost"))]);
        let managed_keys = HashSet::from([String::from("APP_URL")]);

        let result = write_env_values_with_purge(&env_path, &values, true, &managed_keys, true);
        assert!(result.is_err());
    }

    #[test]
    fn write_env_values_full_escapes_special_characters() -> anyhow::Result<()> {
        let env_path = unique_path("escape");
        std::fs::write(&env_path, "").expect("seed env file");

        let values = HashMap::from([(
            String::from("APP_KEY"),
            String::from("with\"quote\\slash\nline"),
        )]);

        write_env_values_full(&env_path, &values)?;
        let content = std::fs::read_to_string(&env_path)?;

        assert_eq!(content, "APP_KEY=\"with\\\"quote\\\\slash\\nline\"\n");
        Ok(())
    }

    fn unique_path(suffix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("helm-env-write-{suffix}-{nanos}.env"))
    }
}
