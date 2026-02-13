//! cli handlers update cmd module.
//!
//! Contains cli handlers update cmd logic used by Helm command workflows.

use anyhow::Result;
use std::path::Path;

use crate::output::{self, LogLevel, Persistence};
use crate::{cli, config, docker, env, serve};

/// Handles the `update` CLI command.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_update(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    profile: Option<&str>,
    force_recreate: bool,
    no_build: bool,
    wait: bool,
    wait_timeout: u64,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<()> {
    let workspace_root = config::project_root_with(config_path, project_root)?;
    let services = cli::support::resolve_up_services(config, service, kind, profile)?;
    for svc in services {
        output::event(
            &svc.name,
            LogLevel::Info,
            "Updating service",
            Persistence::Persistent,
        );
        docker::pull(svc)?;
        if svc.kind == config::Kind::App {
            let inferred_env = env::inferred_app_env(config);
            serve::run(
                svc,
                force_recreate,
                svc.trust_container_ca,
                true,
                &workspace_root,
                &inferred_env,
                !no_build,
            )?;
            if wait {
                serve::wait_until_http_healthy(svc, wait_timeout, 2, None)?;
            }
        } else {
            docker::up(
                svc,
                docker::UpOptions {
                    pull: docker::PullPolicy::Never,
                    recreate: force_recreate,
                },
            )?;
            if wait {
                docker::wait_until_healthy(svc, wait_timeout, 2, None)?;
            }
        }
    }
    Ok(())
}
