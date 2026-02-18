//! Shared package-manager command execution helper.
//!
//! Contains common logic used by `composer` and `node` handlers.

use anyhow::Result;
use std::path::Path;

use crate::{cli, config};

pub(crate) struct HandlePackageManagerCommandOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) manager_bin: &'a str,
    pub(crate) tty: bool,
    pub(crate) no_tty: bool,
    pub(crate) command: &'a [String],
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
    pub(crate) usage_error: &'a str,
}

pub(crate) fn handle_package_manager_command(
    config: &config::Config,
    options: HandlePackageManagerCommandOptions<'_>,
) -> Result<()> {
    if options.command.is_empty() {
        anyhow::bail!("{}", options.usage_error);
    }
    let runtime = cli::support::resolve_app_runtime_context(
        config,
        options.service,
        options.config_path,
        options.project_root,
    )?;
    let mut full_command = vec![options.manager_bin.to_owned()];
    full_command.extend(options.command.iter().cloned());
    let tty = cli::support::effective_tty(options.tty, options.no_tty);
    let start_context = runtime.service_start_context();

    cli::support::run_service_command_with_tty(runtime.target, &full_command, tty, &start_context)
}
