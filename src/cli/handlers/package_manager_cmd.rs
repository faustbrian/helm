//! Shared package-manager command execution helper.
//!
//! Contains common logic used by `composer` and `node` handlers.

use anyhow::Result;
use std::path::Path;

use super::service_scope::selected_services_in_scope;
use crate::{cli, config};

pub(crate) struct HandlePackageManagerCommandOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) kind: Option<config::Kind>,
    pub(crate) profile: Option<&'a str>,
    pub(crate) manager_bin: &'a str,
    pub(crate) non_interactive: bool,
    pub(crate) tty: bool,
    pub(crate) no_tty: bool,
    pub(crate) command: &'a [String],
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
    pub(crate) usage_error: &'a str,
}

pub(crate) fn handle_package_manager_command(
    config: &config::Config,
    options: HandlePackageManagerCommandOptions<'_>,
) -> Result<()> {
    if options.command.is_empty() {
        anyhow::bail!("{}", options.usage_error);
    }
    let selected_service =
        resolve_single_app_service(config, options.service, options.kind, options.profile)?;
    let runtime = cli::support::resolve_app_runtime_context(
        config,
        selected_service.as_deref(),
        options.config_path,
        options.project_root,
    )?;
    let mut full_command = vec![options.manager_bin.to_owned()];
    full_command.extend(options.command.iter().cloned());
    let tty = if options.non_interactive {
        false
    } else {
        cli::support::effective_tty(options.tty, options.no_tty)
    };
    let start_context = runtime.service_start_context();

    cli::support::run_service_command_with_tty(runtime.target, &full_command, tty, &start_context)
}

fn resolve_single_app_service(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    profile: Option<&str>,
) -> Result<Option<String>> {
    if service.is_none() && kind.is_none() && profile.is_none() {
        return Ok(None);
    }
    let mut selected = selected_services_in_scope(config, service, &[], kind, profile)?
        .into_iter()
        .filter(|svc| svc.kind == config::Kind::App)
        .collect::<Vec<_>>();
    if selected.is_empty() {
        anyhow::bail!("no app services matched the requested selector")
    }
    if selected.len() > 1 {
        anyhow::bail!("selector matched multiple app services; use --service to choose one")
    }
    Ok(selected.pop().map(|svc| svc.name.clone()))
}
