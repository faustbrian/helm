//! cli handlers config cmd module.
//!
//! Contains cli handlers config cmd logic used by Helm command workflows.

use anyhow::Result;

use crate::config;

/// Handles the `config` CLI command.
pub(crate) fn handle_config(config: &config::Config, format: &str) -> Result<()> {
    let output = match format {
        "json" => serde_json::to_string_pretty(config)?,
        "toml" => toml::to_string_pretty(config)?,
        _ => anyhow::bail!("unsupported format: {format}"),
    };
    println!("{output}");
    Ok(())
}

/// Handles the `config migrate` CLI command.
pub(crate) fn handle_config_migrate(
    quiet: bool,
    config_path: Option<&std::path::Path>,
    project_root: Option<&std::path::Path>,
) -> Result<()> {
    let path = config::migrate_config_with(config_path, project_root)?;
    if !quiet {
        println!("Migrated {}", path.display());
    }
    Ok(())
}
