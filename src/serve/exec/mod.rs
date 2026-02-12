use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::config::ServiceConfig;

mod container;
mod ensure_artisan_ansi;
mod mysqldump_compat;
mod one_off;
mod runtime;

#[cfg(test)]
pub(crate) use container::build_exec_args;
use container::exec_command;
use ensure_artisan_ansi::ensure_artisan_ansi;
use mysqldump_compat::{
    ensure_mysql_cli_compat, ensure_schema_dump_compat, ensure_schema_dump_compat_for_command,
};
use one_off::run_command_one_off;
pub(crate) use runtime::runtime_cmdline;

/// Executes a Laravel artisan command in the serve container.
///
/// # Errors
///
/// Returns an error if command execution fails.
pub(crate) fn exec_artisan(
    target: &ServiceConfig,
    artisan_args: &[String],
    tty: bool,
) -> Result<()> {
    ensure_mysql_cli_compat(target)?;
    ensure_schema_dump_compat(target, artisan_args)?;
    let mut command = vec!["php".to_owned(), "artisan".to_owned()];
    command.extend(artisan_args.iter().cloned());
    let command = ensure_artisan_ansi(command);
    exec_command(target, &command, tty)
}

/// Executes a command in a running app container, or in a one-off container when not running.
///
/// # Errors
///
/// Returns an error if command execution fails.
pub(crate) fn exec_or_run_command(
    target: &ServiceConfig,
    command: &[String],
    tty: bool,
    project_root: &Path,
    injected_env: &HashMap<String, String>,
) -> Result<()> {
    let command = ensure_artisan_ansi(command.to_vec());
    let container_name = target.container_name()?;
    if let Some(status) = crate::docker::inspect_status(&container_name)
        && status == "running"
    {
        ensure_mysql_cli_compat(target)?;
        ensure_schema_dump_compat_for_command(target, &command, injected_env)?;
        return exec_command(target, &command, tty);
    }

    run_command_one_off(target, &command, tty, project_root, injected_env)
}
