//! cli handlers artisan cmd module.
//!
//! Contains cli handlers artisan cmd logic used by Helm command workflows.

use anyhow::Result;
use std::path::Path;

use crate::{config, env, serve};

mod command;
mod runtime;
mod test_command;
#[cfg(test)]
mod tests;

use command::{
    build_artisan_command, ensure_artisan_ansi_flag, is_artisan_test_command,
    remove_artisan_env_overrides, resolve_artisan_tty,
};
use runtime::ensure_test_services_running;
use test_command::build_artisan_test_command;

/// Handles the `artisan` CLI command.
pub(crate) fn handle_artisan(
    config: &config::Config,
    service: Option<&str>,
    tty: bool,
    no_tty: bool,
    command: &[String],
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<()> {
    if command.is_empty() {
        anyhow::bail!(
            "No artisan command specified. Usage: helm artisan [--service <name>] -- <command>"
        );
    }
    let is_test_command = is_artisan_test_command(command);
    let mut effective_config = config.clone();
    let mut user_command: Vec<String> = command.to_vec();
    if is_test_command {
        config::apply_runtime_env(&mut effective_config, "testing")?;
        user_command = remove_artisan_env_overrides(command);
        user_command.push("--env=testing".to_owned());
    }
    user_command = ensure_artisan_ansi_flag(user_command);

    let workspace_root = config::project_root_with(config_path, project_root)?;
    if is_test_command {
        let test_env = env::inferred_app_env(&effective_config);
        ensure_test_services_running(&mut effective_config, &workspace_root, &test_env)?;
    }
    let serve_target = config::resolve_app_service(&effective_config, service)?;
    let inferred_env = env::inferred_app_env(&effective_config);
    let full_command = if is_test_command {
        build_artisan_test_command(user_command, &inferred_env)
    } else {
        build_artisan_command(user_command)
    };
    serve::exec_or_run_command(
        serve_target,
        &full_command,
        resolve_artisan_tty(tty, no_tty, is_test_command),
        &workspace_root,
        &inferred_env,
    )
}
