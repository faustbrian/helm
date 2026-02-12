use anyhow::Result;
use std::path::PathBuf;

use crate::cli::args::EnvCommands;
use crate::{cli, config};

mod generate;
mod managed;
mod update;

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_env(
    config: &mut config::Config,
    command: Option<&EnvCommands>,
    service: Option<&str>,
    kind: Option<config::Kind>,
    env_file: &Option<PathBuf>,
    sync: bool,
    purge: bool,
    persist_runtime: bool,
    create_missing: bool,
    quiet: bool,
    config_path: &Option<PathBuf>,
    project_root: &Option<PathBuf>,
    runtime_env: Option<&str>,
) -> Result<()> {
    if let Some(EnvCommands::Generate { output }) = command {
        return generate::handle_generate_env(config, output, quiet);
    }

    let env_path =
        cli::support::default_env_path(config_path, project_root, env_file, runtime_env)?;

    if sync || purge || persist_runtime {
        return managed::handle_managed_env_update(
            config,
            service,
            kind,
            &env_path,
            sync,
            purge,
            persist_runtime,
            create_missing,
            quiet,
            config_path,
            project_root,
        );
    }

    update::handle_service_env_update(config, service, kind, &env_path, create_missing, quiet)
}
