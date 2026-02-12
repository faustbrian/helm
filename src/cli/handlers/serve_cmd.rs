use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::{cli, config, env, serve};

pub(crate) fn handle_serve(
    config: &config::Config,
    target: Option<&str>,
    recreate: bool,
    detached: bool,
    env_output: bool,
    trust_container_ca: bool,
    runtime_env: Option<&str>,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
    config_path_buf: &Option<PathBuf>,
    project_root_buf: &Option<PathBuf>,
) -> Result<()> {
    let serve_target = config::resolve_app_service(config, target)?;
    let workspace_root = config::project_root_with(config_path, project_root)?;
    let inferred_env = env::inferred_app_env(config);
    if env_output {
        let serve_env_path =
            cli::support::default_env_path(config_path_buf, project_root_buf, &None, runtime_env)?;
        env::write_env_values(&serve_env_path, &inferred_env, true)?;
    }
    let effective_trust_container_ca = trust_container_ca || serve_target.trust_container_ca;
    serve::run(
        serve_target,
        recreate,
        effective_trust_container_ca,
        detached,
        &workspace_root,
        &inferred_env,
        true,
    )
}
