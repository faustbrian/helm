//! cli handlers relabel cmd module.
//!
//! Contains cli handlers relabel cmd logic used by Helm command workflows.

use anyhow::Result;
use std::path::Path;

use crate::output::{self, LogLevel, Persistence};
use crate::{cli, config, docker, env, serve};

/// Handles the `relabel` CLI command.
pub(crate) fn handle_relabel(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    wait: bool,
    wait_timeout: u64,
    parallel: usize,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<()> {
    let workspace_root = config::project_root_with(config_path, project_root)?;
    let inferred_env = env::inferred_app_env(config);

    cli::support::for_each_service(config, service, kind, None, parallel, |svc| {
        let container_name = svc.container_name()?;
        let Some(status) = docker::inspect_status(&container_name) else {
            output::event(
                &svc.name,
                LogLevel::Info,
                &format!("Skipped relabel because container {container_name} was not found"),
                Persistence::Persistent,
            );
            return Ok(());
        };

        let managed = docker::inspect_label(&container_name, docker::LABEL_MANAGED)
            .is_some_and(|value| value == docker::VALUE_MANAGED_TRUE);

        if managed {
            output::event(
                &svc.name,
                LogLevel::Info,
                &format!(
                    "Skipped relabel because container {container_name} already has Helm labels"
                ),
                Persistence::Persistent,
            );
            return Ok(());
        }

        output::event(
            &svc.name,
            LogLevel::Info,
            &format!(
                "Recreating container {container_name} to apply Helm labels (status: {status})"
            ),
            Persistence::Persistent,
        );

        if svc.kind == config::Kind::App {
            serve::run(
                svc,
                true,
                svc.trust_container_ca,
                true,
                &workspace_root,
                &inferred_env,
                true,
            )?;
            if wait {
                serve::wait_until_http_healthy(svc, wait_timeout, 2, None)?;
            }
        } else {
            docker::recreate(svc)?;
            docker::up(
                svc,
                docker::UpOptions {
                    pull: docker::PullPolicy::Missing,
                    recreate: false,
                },
            )?;
            if wait {
                docker::wait_until_healthy(svc, wait_timeout, 2, None)?;
            }
        }

        output::event(
            &svc.name,
            LogLevel::Success,
            &format!("Applied Helm labels to container {container_name}"),
            Persistence::Persistent,
        );
        Ok(())
    })
}
