//! Lifecycle hook execution for selected services.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::config::{HookOnError, HookPhase, HookRun, ServiceConfig, ServiceHook};
use crate::output::{self, LogLevel, Persistence};

pub(crate) fn run_phase_hooks_for_services(
    services: &[&ServiceConfig],
    phase: HookPhase,
    workspace_root: &Path,
    quiet: bool,
) -> Result<()> {
    run_phase_hooks_with_executor(services, phase, workspace_root, quiet, execute_hook)
}

fn run_phase_hooks_with_executor<F>(
    services: &[&ServiceConfig],
    phase: HookPhase,
    workspace_root: &Path,
    quiet: bool,
    mut executor: F,
) -> Result<()>
where
    F: FnMut(&ServiceConfig, &ServiceHook, HookPhase, &Path) -> Result<()>,
{
    for service in services {
        for hook in &service.hook {
            if hook.phase != phase {
                continue;
            }

            if !quiet {
                output::event(
                    &service.name,
                    LogLevel::Info,
                    &format!("Running hook '{}' ({})", hook.name, hook_phase_label(phase)),
                    Persistence::Persistent,
                );
            }

            let result = executor(service, hook, phase, workspace_root).with_context(|| {
                format!("hook '{}' failed for service '{}'", hook.name, service.name)
            });

            match result {
                Ok(()) => {
                    if !quiet {
                        output::event(
                            &service.name,
                            LogLevel::Success,
                            &format!("Hook '{}' completed", hook.name),
                            Persistence::Persistent,
                        );
                    }
                }
                Err(error) => {
                    if hook.on_error == HookOnError::Warn {
                        output::event(
                            &service.name,
                            LogLevel::Warn,
                            &format!(
                                "Hook '{}' failed but continuing (`on_error=warn`): {error}",
                                hook.name
                            ),
                            Persistence::Persistent,
                        );
                        continue;
                    }
                    return Err(error);
                }
            }
        }
    }

    Ok(())
}

fn execute_hook(
    service: &ServiceConfig,
    hook: &ServiceHook,
    phase: HookPhase,
    workspace_root: &Path,
) -> Result<()> {
    match &hook.run {
        HookRun::Exec { argv } => run_exec_hook(service, argv, phase),
        HookRun::Script { path } => run_script_hook(path, hook.timeout_sec, workspace_root),
    }
}

fn run_exec_hook(service: &ServiceConfig, argv: &[String], phase: HookPhase) -> Result<()> {
    if phase == HookPhase::PostDown {
        anyhow::bail!("`run.type=exec` is not supported for post_down hooks");
    }
    if argv.is_empty() {
        anyhow::bail!("`run.argv` must include at least one command token");
    }
    crate::docker::exec_command(service, argv, false)
}

fn run_script_hook(path: &str, timeout_sec: Option<u64>, workspace_root: &Path) -> Result<()> {
    let resolved = resolve_script_path(path, workspace_root);
    let command_display = format!("sh {}", resolved.display());

    if crate::docker::is_dry_run() {
        output::event(
            "hooks",
            LogLevel::Info,
            &format!("[dry-run] {command_display}"),
            Persistence::Transient,
        );
        return Ok(());
    }

    let mut child = Command::new("sh")
        .arg(&resolved)
        .current_dir(workspace_root)
        .spawn()
        .with_context(|| format!("failed to execute hook script at {}", resolved.display()))?;

    if let Some(timeout) = timeout_sec {
        wait_with_timeout(&mut child, Duration::from_secs(timeout), &command_display)?;
    } else {
        let status = child
            .wait()
            .with_context(|| format!("failed waiting for hook script {}", resolved.display()))?;
        if !status.success() {
            anyhow::bail!(
                "hook script exited with non-zero status: {}",
                resolved.display()
            );
        }
    }

    Ok(())
}

fn wait_with_timeout(
    child: &mut std::process::Child,
    timeout: Duration,
    command_display: &str,
) -> Result<()> {
    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .context("failed while polling hook script")?
        {
            if status.success() {
                return Ok(());
            }
            anyhow::bail!("hook command exited with non-zero status: {command_display}");
        }

        if started.elapsed() > timeout {
            child
                .kill()
                .context("failed to terminate timed-out hook script")?;
            drop(child.wait());
            anyhow::bail!(
                "hook command timed out after {}s: {command_display}",
                timeout.as_secs()
            );
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

fn resolve_script_path(path: &str, workspace_root: &Path) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        return path;
    }
    workspace_root.join(path)
}

const fn hook_phase_label(phase: HookPhase) -> &'static str {
    match phase {
        HookPhase::PostUp => "post_up",
        HookPhase::PreDown => "pre_down",
        HookPhase::PostDown => "post_down",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Driver, Kind};

    fn app_service_with_hooks(hook: Vec<ServiceHook>) -> ServiceConfig {
        ServiceConfig {
            name: "app".to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
            image: "dunglas/frankenphp:php8.5".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 8000,
            database: None,
            username: None,
            password: None,
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: None,
            domain: Some("app.localhost".to_owned()),
            domains: None,
            container_port: None,
            smtp_port: None,
            volumes: None,
            env: None,
            command: None,
            depends_on: None,
            seed_file: None,
            hook,
            health_path: None,
            health_statuses: None,
            localhost_tls: false,
            octane: false,
            php_extensions: None,
            trust_container_ca: false,
            env_mapping: None,
            container_name: Some("test-app".to_owned()),
            resolved_container_name: None,
        }
    }

    #[test]
    fn run_phase_executes_only_matching_hooks() {
        let service = app_service_with_hooks(vec![
            ServiceHook {
                name: "one".to_owned(),
                phase: HookPhase::PostUp,
                run: HookRun::Exec {
                    argv: vec!["php".to_owned(), "-v".to_owned()],
                },
                on_error: HookOnError::Fail,
                timeout_sec: None,
            },
            ServiceHook {
                name: "two".to_owned(),
                phase: HookPhase::PostDown,
                run: HookRun::Script {
                    path: ".helm/hooks/two.sh".to_owned(),
                },
                on_error: HookOnError::Fail,
                timeout_sec: None,
            },
        ]);
        let selected = vec![&service];
        let mut executed = Vec::new();

        run_phase_hooks_with_executor(
            &selected,
            HookPhase::PostUp,
            Path::new("/tmp/work"),
            true,
            |_, hook, _, _| {
                executed.push(hook.name.clone());
                Ok(())
            },
        )
        .expect("run post_up hooks");

        assert_eq!(executed, vec!["one".to_owned()]);
    }

    #[test]
    fn run_phase_warns_and_continues_when_on_error_warn() {
        let service = app_service_with_hooks(vec![
            ServiceHook {
                name: "warn-hook".to_owned(),
                phase: HookPhase::PostUp,
                run: HookRun::Exec {
                    argv: vec!["php".to_owned(), "-v".to_owned()],
                },
                on_error: HookOnError::Warn,
                timeout_sec: None,
            },
            ServiceHook {
                name: "next".to_owned(),
                phase: HookPhase::PostUp,
                run: HookRun::Exec {
                    argv: vec!["php".to_owned(), "-m".to_owned()],
                },
                on_error: HookOnError::Fail,
                timeout_sec: None,
            },
        ]);
        let selected = vec![&service];
        let mut executed = Vec::new();

        run_phase_hooks_with_executor(
            &selected,
            HookPhase::PostUp,
            Path::new("/tmp/work"),
            true,
            |_, hook, _, _| {
                executed.push(hook.name.clone());
                if hook.name == "warn-hook" {
                    anyhow::bail!("boom");
                }
                Ok(())
            },
        )
        .expect("warn hook should not fail command");

        assert_eq!(executed, vec!["warn-hook".to_owned(), "next".to_owned()]);
    }

    #[test]
    fn run_phase_fails_when_on_error_fail() {
        let service = app_service_with_hooks(vec![
            ServiceHook {
                name: "fail-hook".to_owned(),
                phase: HookPhase::PostUp,
                run: HookRun::Exec {
                    argv: vec!["php".to_owned(), "-v".to_owned()],
                },
                on_error: HookOnError::Fail,
                timeout_sec: None,
            },
            ServiceHook {
                name: "next".to_owned(),
                phase: HookPhase::PostUp,
                run: HookRun::Exec {
                    argv: vec!["php".to_owned(), "-m".to_owned()],
                },
                on_error: HookOnError::Fail,
                timeout_sec: None,
            },
        ]);
        let selected = vec![&service];
        let mut executed = Vec::new();

        let error = run_phase_hooks_with_executor(
            &selected,
            HookPhase::PostUp,
            Path::new("/tmp/work"),
            true,
            |_, hook, _, _| {
                executed.push(hook.name.clone());
                anyhow::bail!("boom");
            },
        )
        .expect_err("fail hook should stop command");

        assert!(error.to_string().contains("hook 'fail-hook' failed"));
        assert_eq!(executed, vec!["fail-hook".to_owned()]);
    }

    #[test]
    fn resolve_script_path_joins_workspace_root_for_relative_paths() {
        let root = Path::new("/tmp/workspace");
        let resolved = resolve_script_path(".helm/hooks/seed.sh", root);
        assert_eq!(
            resolved,
            PathBuf::from("/tmp/workspace/.helm/hooks/seed.sh")
        );
    }
}
