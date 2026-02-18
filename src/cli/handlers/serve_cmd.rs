//! cli handlers serve cmd module.
//!
//! Contains cli handlers serve cmd logic used by Helm command workflows.

use anyhow::Result;
use std::path::Path;

use crate::{cli, config, env, serve};

pub(crate) struct HandleServeOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) recreate: bool,
    pub(crate) detached: bool,
    pub(crate) env_output: bool,
    pub(crate) trust_container_ca: bool,
    pub(crate) runtime_env: Option<&'a str>,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
}

pub(crate) fn handle_serve(config: &config::Config, options: HandleServeOptions<'_>) -> Result<()> {
    let runtime = cli::support::resolve_app_runtime_context(
        config,
        options.service,
        options.config_path,
        options.project_root,
    )?;
    if let Some(serve_env_path) = cli::support::env_output_path(
        options.env_output,
        options.config_path,
        options.project_root,
        options.runtime_env,
    )? {
        env::write_env_values(&serve_env_path, &runtime.app_env, true)?;
    }
    let effective_trust_container_ca =
        options.trust_container_ca || runtime.target.trust_container_ca;
    serve::run(serve::RunServeOptions {
        target: runtime.target,
        recreate: options.recreate,
        trust_container_ca: effective_trust_container_ca,
        detached: options.detached,
        project_root: &runtime.workspace_root,
        injected_env: &runtime.app_env,
        allow_rebuild: true,
    })
}
