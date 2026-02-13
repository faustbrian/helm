//! cli handlers env cmd generate module.
//!
//! Contains cli handlers env cmd generate logic used by Helm command workflows.

use anyhow::Result;
use std::path::Path;

use crate::{config, env};

/// Handles the `generate env` CLI command.
pub(super) fn handle_generate_env(
    config: &config::Config,
    output: &Path,
    quiet: bool,
) -> Result<()> {
    let values = env::managed_app_env(config);
    env::write_env_values_full(output, &values)?;
    if !quiet {
        println!("Generated {} with {} keys", output.display(), values.len());
    }
    Ok(())
}
