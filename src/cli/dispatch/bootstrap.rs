use anyhow::Result;
use clap::CommandFactory;
use clap_complete::generate;

use crate::cli::args::{Cli, Commands};
use crate::config::{self, Config};
use crate::output::{self, LogLevel, Persistence};

pub(super) fn handle_setup_commands(cli: &Cli) -> Result<bool> {
    if matches!(&cli.command, Commands::Init) {
        let path = config::init_config()?;
        if !cli.quiet {
            output::event(
                "init",
                LogLevel::Success,
                &format!("Created {}", path.display()),
                Persistence::Persistent,
            );
        }
        return Ok(true);
    }

    if let Commands::Completions(args) = &cli.command {
        let mut cmd = Cli::command();
        generate(args.shell, &mut cmd, "helm", &mut std::io::stdout());
        return Ok(true);
    }

    Ok(false)
}

pub(super) fn load_config_for_cli(cli: &Cli) -> Result<Config> {
    let mut config = if cli.config.is_none() && cli.project_root.is_none() {
        config::load_config()?
    } else {
        config::load_config_with(cli.config.as_deref(), cli.project_root.as_deref())?
    };
    if let Some(runtime_env) = cli.env.as_deref() {
        config::apply_runtime_env(&mut config, runtime_env)?;
    }
    Ok(config)
}
