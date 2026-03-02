//! start command bootstrap helpers.

use anyhow::Result;
use std::path::Path;

use crate::cli::handlers::log;
use crate::{cli, config};

pub(super) struct StartBootstrapOptions<'a> {
    pub(super) service: Option<&'a str>,
    pub(super) kind: Option<config::Kind>,
    pub(super) profile: Option<&'a str>,
    pub(super) runtime_env: Option<&'a str>,
    pub(super) config_path: Option<&'a Path>,
    pub(super) project_root: Option<&'a Path>,
    pub(super) quiet: bool,
}

pub(super) fn run_start_bootstrap(
    config: &config::Config,
    options: StartBootstrapOptions<'_>,
) -> Result<()> {
    let selected =
        cli::support::resolve_up_services(config, options.service, options.kind, options.profile)?;
    let Some(target) = select_bootstrap_target(&selected) else {
        log::info_if_not_quiet(
            options.quiet,
            "start",
            "Skipping Laravel bootstrap because no web app target was selected",
        );
        return Ok(());
    };

    let runtime = cli::support::resolve_project_runtime_context(
        config,
        options.config_path,
        options.project_root,
    )?;
    let env_path = cli::support::default_env_path(
        options.config_path,
        options.project_root,
        None,
        options.runtime_env,
    )?;
    let mut bootstrap_commands = vec![storage_link_command(), migrate_command()];
    if app_key_missing(&env_path) {
        bootstrap_commands.insert(0, key_generate_command());
    }
    let start_context = runtime.service_start_context();

    cli::support::run_service_commands(target, &bootstrap_commands, &start_context)?;

    Ok(())
}

pub(super) fn select_bootstrap_target<'a>(
    selected: &[&'a config::ServiceConfig],
) -> Option<&'a config::ServiceConfig> {
    selected.iter().copied().find(|svc| {
        svc.kind == config::Kind::App
            && svc.driver == config::Driver::Frankenphp
            && svc.command.is_none()
    })
}

pub(super) fn app_key_missing(env_path: &Path) -> bool {
    let Ok(contents) = std::fs::read_to_string(env_path) else {
        return true;
    };

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if key.trim() != "APP_KEY" {
            continue;
        }
        return value.trim().is_empty();
    }

    true
}

fn key_generate_command() -> Vec<String> {
    cli::support::build_artisan_subcommand("key:generate", &["--ansi", "--force"])
}

fn storage_link_command() -> Vec<String> {
    cli::support::build_artisan_subcommand("storage:link", &["--ansi", "--force"])
}

fn migrate_command() -> Vec<String> {
    cli::support::build_artisan_subcommand("migrate", &["--isolated", "--ansi", "--force"])
}
