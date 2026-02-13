//! cli handlers start cmd module.
//!
//! Contains cli handlers start cmd logic used by Helm command workflows.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::output::{self, LogLevel, Persistence};
use crate::{config, docker};

use super::{handle_open, handle_up};

/// Handles the `start` CLI command.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_start(
    config: &mut config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    profile: Option<&str>,
    wait: bool,
    no_wait: bool,
    wait_timeout: u64,
    pull_policy: docker::PullPolicy,
    force_recreate: bool,
    open_after_start: bool,
    health_path: Option<&str>,
    include_project_deps: bool,
    parallel: usize,
    quiet: bool,
    no_color: bool,
    dry_run: bool,
    repro: bool,
    runtime_env: Option<&str>,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
    config_path_buf: &Option<PathBuf>,
    project_root_buf: &Option<PathBuf>,
) -> Result<()> {
    crate::cli::support::run_doctor(config, false, repro, false, true, config_path, project_root)?;

    handle_up(
        config,
        service,
        kind,
        profile,
        wait,
        no_wait,
        wait_timeout,
        pull_policy,
        force_recreate,
        false,
        false,
        crate::cli::args::PortStrategyArg::Random,
        None,
        false,
        false,
        include_project_deps,
        false,
        parallel,
        quiet,
        no_color,
        dry_run,
        repro,
        runtime_env,
        config_path,
        project_root,
        config_path_buf,
        project_root_buf,
    )?;

    if !open_after_start {
        return Ok(());
    }

    let only_non_app_kind = kind.is_some_and(|value| value != config::Kind::App);
    if only_non_app_kind {
        if !quiet {
            output::event(
                "start",
                LogLevel::Info,
                "Skipping app URL summary because selected kind has no app services",
                Persistence::Persistent,
            );
        }
        return Ok(());
    }

    if let Some(service_name) = service {
        let selected = config::find_service(config, service_name)?;
        if selected.kind != config::Kind::App {
            if !quiet {
                output::event(
                    "start",
                    LogLevel::Info,
                    &format!(
                        "Skipping app URL summary because '{}' is not an app service",
                        selected.name
                    ),
                    Persistence::Persistent,
                );
            }
            return Ok(());
        }
        return handle_open(config, Some(service_name), false, health_path, false, false);
    }

    handle_open(config, None, true, health_path, false, false)
}
