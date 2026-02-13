//! Top-level CLI dispatch pipeline.
//!
//! This is the single entrypoint that applies global CLI process settings
//! (`--no-color`, `--quiet`, `--dry-run`), runs setup-only commands that do not
//! require config loading, and then routes to primary/secondary command trees.

use anyhow::Result;

use super::args::Cli;
use crate::docker;
use crate::output::{self, LoggerOptions};

mod bootstrap;
mod primary;
mod secondary;

/// Executes one full CLI invocation.
///
/// Dispatch order matters:
/// 1. Apply global process-level flags.
/// 2. Handle setup commands that can run without config.
/// 3. Load and optionally runtime-patch config.
/// 4. Attempt primary dispatch, then fall through to secondary.
pub(crate) fn run(cli: Cli) -> Result<()> {
    if cli.no_color {
        colored::control::set_override(false);
    }

    output::init(LoggerOptions::new(cli.quiet));
    docker::set_dry_run(cli.dry_run);

    if bootstrap::handle_setup_commands(&cli)? {
        return Ok(());
    }

    let mut config = bootstrap::load_config_for_cli(&cli)?;
    if let Some(result) = primary::dispatch_primary(&cli, &mut config) {
        return result;
    }
    secondary::dispatch_secondary(&cli, &mut config)
}
