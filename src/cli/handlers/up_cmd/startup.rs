use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::output::{self, LogLevel, Persistence};
use crate::{cli, config, docker, env, serve};

#[allow(clippy::too_many_arguments)]
pub(super) fn run_standard_up(
    config: &mut config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    profile: Option<&str>,
    healthy: bool,
    timeout: u64,
    pull_policy: docker::PullPolicy,
    recreate: bool,
    quiet: bool,
    workspace_root: &Path,
    project_dependency_env: &HashMap<String, String>,
) -> Result<()> {
    let startup_services = cli::support::resolve_up_services(config, service, kind, profile)?;
    for svc in startup_services {
        if !quiet {
            output::event(
                &svc.name,
                LogLevel::Info,
                "Starting service",
                Persistence::Persistent,
            );
        }
        if svc.kind == config::Kind::App {
            let mut injected_env = env::inferred_app_env(config);
            injected_env.extend(project_dependency_env.clone());
            serve::run(
                svc,
                recreate,
                svc.trust_container_ca,
                true,
                workspace_root,
                &injected_env,
                true,
            )?;
            if healthy {
                serve::wait_until_http_healthy(svc, timeout, 2, None)?;
            }
        } else {
            docker::up(
                svc,
                docker::UpOptions {
                    pull: pull_policy,
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
