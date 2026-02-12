use anyhow::Result;
use std::path::Path;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::{read_env_lines, write_env_lines};
use crate::env::mapping::build_laravel_env_map;

/// Updates the .env file at `env_path` with values from service config.
///
/// Replaces existing environment variable lines in-place. With
/// `create_missing=true`, missing vars are appended.
///
/// # Errors
///
/// Returns an error if the .env file cannot be read or written.
pub(crate) fn update_env(
    service: &ServiceConfig,
    env_path: &Path,
    create_missing: bool,
) -> Result<()> {
    let mut lines = read_env_lines(env_path)?;
    let mut updated: Vec<String> = Vec::new();

    let var_map = build_laravel_env_map(service);

    for line in &mut lines {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }

        if let Some((var_name, value)) = var_map.iter().find(|(var_name, _)| {
            trimmed.starts_with(var_name.as_str())
                && trimmed
                    .get(var_name.len()..)
                    .is_some_and(|s| s.starts_with('='))
        }) {
            *line = format!("{var_name}=\"{value}\"");
            updated.push(var_name.clone());
        }
    }

    if create_missing {
        for (var_name, value) in &var_map {
            if !updated.iter().any(|existing| existing == var_name) {
                lines.push(format!("{var_name}=\"{value}\""));
                updated.push(var_name.clone());
            }
        }
    }

    write_env_lines(env_path, lines)?;

    if updated.is_empty() {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("No matching variables found in {}", env_path.display()),
            Persistence::Persistent,
        );
    } else {
        for var in updated {
            output::event(
                &service.name,
                LogLevel::Success,
                &format!("Updated env variable {var}"),
                Persistence::Persistent,
            );
        }
    }

    Ok(())
}
