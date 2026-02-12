use anyhow::Result;

use super::args::Cli;
use crate::docker;
use crate::output::{self, LoggerOptions};

mod bootstrap;
mod primary;
mod secondary;

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
