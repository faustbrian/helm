//! Primary command dispatch.
//!
//! This layer handles config/preset/profile/doctor/lock and delegates operational
//! commands to `operations`.

use anyhow::Result;

use crate::cli::args::{Cli, Commands, ConfigCommands, PresetCommands, ProfileCommands};
use crate::cli::{handlers, support};
use crate::config::Config;

mod operations;

use operations::dispatch_operation_commands;

/// Attempts to dispatch commands owned by the primary tree.
///
/// Returns `Some(result)` when the command was handled, otherwise `None` so the
/// secondary tree can attempt dispatch.
pub(super) fn dispatch_primary(cli: &Cli, config: &mut Config) -> Option<Result<()>> {
    match &cli.command {
        Commands::Init | Commands::Completions(_) => Some(Ok(())),
        Commands::Config(args) => Some(match args.command {
            Some(ConfigCommands::Migrate) => handlers::handle_config_migrate(
                cli.quiet,
                cli.config.as_deref(),
                cli.project_root.as_deref(),
            ),
            None => handlers::handle_config(config, &args.format),
        }),
        Commands::Preset(args) => Some(match &args.command {
            PresetCommands::List => {
                handlers::handle_preset_list();
                Ok(())
            }
            PresetCommands::Show { name, format } => handlers::handle_preset_show(name, format),
        }),
        Commands::Profile(args) => Some(match &args.command {
            ProfileCommands::List => {
                handlers::handle_profile_list();
                Ok(())
            }
            ProfileCommands::Show { name, format } => {
                handlers::handle_profile_show(config, name, format)
            }
        }),
        Commands::Doctor(args) => Some(support::run_doctor(
            config,
            args.fix,
            args.repro,
            args.reachability,
            false,
            cli.config.as_deref(),
            cli.project_root.as_deref(),
        )),
        Commands::Lock(args) => Some(handlers::handle_lock(
            config,
            &args.command,
            cli.quiet,
            cli.config.as_deref(),
            cli.project_root.as_deref(),
        )),
        _ => dispatch_operation_commands(cli, config),
    }
}
