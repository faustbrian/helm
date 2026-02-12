use anyhow::Result;
use std::path::Path;

use crate::{config, env, serve};

mod commands;

pub(crate) fn handle_app_create(
    config: &config::Config,
    target: Option<&str>,
    no_migrate: bool,
    seed: bool,
    no_storage_link: bool,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<()> {
    let serve_target = config::resolve_app_service(config, target)?;
    let workspace_root = config::project_root_with(config_path, project_root)?;
    let inferred_env = env::inferred_app_env(config);

    for command in commands::setup_commands() {
        serve::exec_or_run_command(
            serve_target,
            &command,
            false,
            &workspace_root,
            &inferred_env,
        )?;
    }

    if !no_storage_link {
        serve::exec_or_run_command(
            serve_target,
            &commands::storage_link_command(),
            false,
            &workspace_root,
            &inferred_env,
        )?;
    }

    if !no_migrate {
        serve::exec_or_run_command(
            serve_target,
            &commands::migrate_command(),
            false,
            &workspace_root,
            &inferred_env,
        )?;
    }

    if seed {
        serve::exec_or_run_command(
            serve_target,
            &commands::seed_command(),
            false,
            &workspace_root,
            &inferred_env,
        )?;
    }

    Ok(())
}
