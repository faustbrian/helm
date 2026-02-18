//! cli handlers artisan cmd module.
//!
//! Contains cli handlers artisan cmd logic used by Helm command workflows.

use anyhow::{Result, anyhow};
use std::path::Path;

use crate::{cli, config};

mod command;
mod runtime;
mod test_command;
#[cfg(test)]
mod tests;

use command::{
    build_artisan_command, ensure_artisan_ansi_flag, is_artisan_test_command,
    remove_artisan_env_overrides, resolve_artisan_tty,
};
use runtime::{cleanup_test_services, ensure_test_services_running, testing_runtime_env_name};
use test_command::build_artisan_test_command;

pub(crate) struct HandleArtisanOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) tty: bool,
    pub(crate) no_tty: bool,
    pub(crate) command: &'a [String],
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
}

pub(crate) fn handle_artisan(
    config: &config::Config,
    options: HandleArtisanOptions<'_>,
) -> Result<()> {
    if options.command.is_empty() {
        anyhow::bail!(
            "No artisan command specified. Usage: helm artisan [--service <name>] -- <command>"
        );
    }
    let is_test_command = is_artisan_test_command(options.command);
    let mut effective_config = config.clone();
    let mut user_command: Vec<String> = options.command.to_vec();
    if is_test_command {
        let runtime_env = testing_runtime_env_name();
        config::apply_runtime_env(&mut effective_config, &runtime_env)?;
        user_command = remove_artisan_env_overrides(options.command);
        user_command.push("--env=testing".to_owned());
    }
    user_command = ensure_artisan_ansi_flag(user_command);

    let mut workspace_root = None;
    let mut prepared_test_runtime = false;
    if is_test_command {
        let resolved_workspace_root =
            cli::support::workspace_root(options.config_path, options.project_root)?;
        match ensure_test_services_running(&mut effective_config, &resolved_workspace_root) {
            Ok(()) => prepared_test_runtime = true,
            Err(start_error) => {
                return match cleanup_test_services(&effective_config) {
                    Ok(()) => Err(start_error),
                    Err(cleanup_error) => Err(anyhow!(
                        "failed to prepare test runtime: {start_error}; failed to cleanup test \
                         runtime: {cleanup_error}"
                    )),
                };
            }
        }
        workspace_root = Some(resolved_workspace_root);
    }
    let runtime = if let Some(workspace_root) = workspace_root {
        cli::support::resolve_app_runtime_context_with_workspace_root(
            &effective_config,
            options.service,
            workspace_root,
        )?
    } else {
        cli::support::resolve_app_runtime_context(
            &effective_config,
            options.service,
            options.config_path,
            options.project_root,
        )?
    };
    let full_command = if is_test_command {
        build_artisan_test_command(user_command, &runtime.app_env)
    } else {
        build_artisan_command(user_command)
    };
    let start_context = runtime.service_start_context();
    let command_result = cli::support::run_service_command_with_tty(
        runtime.target,
        &full_command,
        resolve_artisan_tty(options.tty, options.no_tty, is_test_command),
        &start_context,
    );

    if !(is_test_command && prepared_test_runtime) {
        return command_result;
    }

    let cleanup_result = cleanup_test_services(&effective_config);
    match (command_result, cleanup_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(command_error), Ok(())) => Err(command_error),
        (Ok(()), Err(cleanup_error)) => Err(cleanup_error),
        (Err(command_error), Err(cleanup_error)) => Err(anyhow!(
            "artisan test command failed: {command_error}; test runtime cleanup failed: \
             {cleanup_error}"
        )),
    }
}
