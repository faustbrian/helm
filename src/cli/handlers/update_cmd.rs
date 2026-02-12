use anyhow::Result;
use std::path::Path;

use crate::output::{self, LogLevel, Persistence};
use crate::{cli, config, docker, env, serve};

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_update(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    profile: Option<&str>,
    recreate: bool,
    no_rebuild_app_image: bool,
    healthy: bool,
    timeout: u64,
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
                recreate,
                svc.trust_container_ca,
                true,
                &workspace_root,
                &inferred_env,
                !no_rebuild_app_image,
            )?;
            if healthy {
                serve::wait_until_http_healthy(svc, timeout, 2, None)?;
            }
        } else {
            docker::up(
                svc,
                docker::UpOptions {
                    pull: docker::PullPolicy::Never,
                    recreate,
                },
            )?;
            if healthy {
                docker::wait_until_healthy(svc, timeout, 2, None)?;
            }
        }
    }
    Ok(())
}
