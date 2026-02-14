//! cli handlers start cmd module.
//!
//! Contains cli handlers start cmd logic used by Helm command workflows.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::output::{self, LogLevel, Persistence};
use crate::{cli, config, docker, env, serve};

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
    let (effective_wait, effective_no_wait) = resolve_start_wait_flags(wait, no_wait);

    cli::support::run_doctor(config, false, repro, false, true, config_path, project_root)?;

    handle_up(
        config,
        service,
        kind,
        profile,
        effective_wait,
        effective_no_wait,
        wait_timeout,
        pull_policy,
        force_recreate,
        false,
        false,
        cli::args::PortStrategyArg::Random,
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

    run_start_bootstrap(
        config,
        service,
        kind,
        profile,
        runtime_env,
        config_path_buf,
        project_root_buf,
        quiet,
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

fn resolve_start_wait_flags(wait: bool, no_wait: bool) -> (bool, bool) {
    (wait, no_wait || !wait)
}

fn run_start_bootstrap(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    profile: Option<&str>,
    runtime_env: Option<&str>,
    config_path: &Option<PathBuf>,
    project_root: &Option<PathBuf>,
    quiet: bool,
) -> Result<()> {
    let selected = cli::support::resolve_up_services(config, service, kind, profile)?;
    let Some(target) = select_bootstrap_target(&selected) else {
        if !quiet {
            output::event(
                "start",
                LogLevel::Info,
                "Skipping Laravel bootstrap because no web app target was selected",
                Persistence::Persistent,
            );
        }
        return Ok(());
    };

    let workspace_root =
        config::project_root_with(config_path.as_deref(), project_root.as_deref())?;
    let inferred_env = env::inferred_app_env(config);
    let env_path = cli::support::default_env_path(config_path, project_root, &None, runtime_env)?;
    let should_generate_key = app_key_missing(&env_path);

    if should_generate_key {
        serve::exec_or_run_command(
            target,
            &key_generate_command(),
            false,
            &workspace_root,
            &inferred_env,
        )?;
    }
    serve::exec_or_run_command(
        target,
        &storage_link_command(),
        false,
        &workspace_root,
        &inferred_env,
    )?;
    serve::exec_or_run_command(
        target,
        &migrate_command(),
        false,
        &workspace_root,
        &inferred_env,
    )?;

    Ok(())
}

fn select_bootstrap_target<'a>(
    selected: &[&'a config::ServiceConfig],
) -> Option<&'a config::ServiceConfig> {
    selected.iter().copied().find(|svc| {
        svc.kind == config::Kind::App
            && svc.driver == config::Driver::Frankenphp
            && svc.command.is_none()
    })
}

fn app_key_missing(env_path: &Path) -> bool {
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
    vec![
        "php".to_owned(),
        "artisan".to_owned(),
        "key:generate".to_owned(),
        "--ansi".to_owned(),
        "--force".to_owned(),
    ]
}

fn storage_link_command() -> Vec<String> {
    vec![
        "php".to_owned(),
        "artisan".to_owned(),
        "storage:link".to_owned(),
        "--ansi".to_owned(),
        "--force".to_owned(),
    ]
}

fn migrate_command() -> Vec<String> {
    vec![
        "php".to_owned(),
        "artisan".to_owned(),
        "migrate".to_owned(),
        "--isolated".to_owned(),
        "--ansi".to_owned(),
        "--force".to_owned(),
    ]
}

#[cfg(test)]
mod tests {
    use super::{app_key_missing, resolve_start_wait_flags, select_bootstrap_target};
    use crate::config::{Driver, Kind, ServiceConfig};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn start_wait_defaults_to_no_wait_when_unspecified() {
        assert_eq!(resolve_start_wait_flags(false, false), (false, true));
    }

    #[test]
    fn start_wait_flag_enables_wait() {
        assert_eq!(resolve_start_wait_flags(true, false), (true, false));
    }

    #[test]
    fn start_no_wait_stays_no_wait() {
        assert_eq!(resolve_start_wait_flags(false, true), (false, true));
    }

    #[test]
    fn app_key_missing_returns_false_when_key_present() {
        let path = temp_env_path("present");
        fs::write(&path, "APP_NAME=Helm\nAPP_KEY=base64:abc123\n").expect("write env");

        assert!(!app_key_missing(&path));

        fs::remove_file(path).expect("cleanup env");
    }

    #[test]
    fn app_key_missing_returns_true_when_key_blank() {
        let path = temp_env_path("blank");
        fs::write(&path, "APP_KEY=\n").expect("write env");

        assert!(app_key_missing(&path));

        fs::remove_file(path).expect("cleanup env");
    }

    fn temp_env_path(suffix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("helm-start-{suffix}-{nanos}.env"))
    }

    #[test]
    fn select_bootstrap_target_prefers_primary_web_app() {
        let horizon = service("horizon", Driver::Horizon, Some(vec!["php".to_owned()]));
        let queue_worker = service(
            "queue-worker",
            Driver::Frankenphp,
            Some(vec!["php".to_owned(), "artisan".to_owned()]),
        );
        let app = service("app", Driver::Frankenphp, None);
        let selected = vec![&horizon, &queue_worker, &app];

        let target = select_bootstrap_target(&selected).expect("bootstrap target");
        assert_eq!(target.name, "app");
    }

    fn service(name: &str, driver: Driver, command: Option<Vec<String>>) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind: Kind::App,
            driver,
            image: "dunglas/frankenphp:php8.5".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 33065,
            database: None,
            username: None,
            password: None,
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: None,
            domain: Some("app.helm".to_owned()),
            domains: None,
            container_port: Some(80),
            smtp_port: None,
            volumes: None,
            env: None,
            command,
            depends_on: None,
            seed_file: None,
            hook: Vec::new(),
            health_path: None,
            health_statuses: None,
            localhost_tls: false,
            octane: false,
            php_extensions: None,
            trust_container_ca: false,
            env_mapping: None,
            container_name: None,
            resolved_container_name: None,
        }
    }
}
