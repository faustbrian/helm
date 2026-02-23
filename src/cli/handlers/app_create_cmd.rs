//! cli handlers app create cmd module.
//!
//! Contains cli handlers app create cmd logic used by Helm command workflows.

use crate::{cli, config};
use anyhow::Result;
use std::path::Path;

mod commands;

pub(crate) struct HandleAppCreateOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) no_migrate: bool,
    pub(crate) seed: bool,
    pub(crate) no_storage_link: bool,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
}

pub(crate) fn handle_app_create(
    config: &config::Config,
    options: HandleAppCreateOptions<'_>,
) -> Result<()> {
    let runtime = cli::support::resolve_app_runtime_context(
        config,
        options.service,
        options.config_path,
        options.project_root,
    )?;
    let start_context = runtime.service_start_context();

    let setup_commands = commands::setup_commands();
    cli::support::run_service_commands(runtime.target, &setup_commands, &start_context)?;

    let mut post_setup_commands = Vec::new();
    if !options.no_storage_link {
        post_setup_commands.push(commands::storage_link_command());
    }

    if !options.no_migrate {
        post_setup_commands.push(commands::migrate_command());
    }

    if options.seed {
        post_setup_commands.push(commands::seed_command());
    }

    cli::support::run_service_commands(runtime.target, &post_setup_commands, &start_context)?;

    Ok(())
}
