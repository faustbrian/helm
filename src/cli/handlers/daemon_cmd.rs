//! cli handlers daemon cmd module.
//!
//! Contains pre-config daemon command routing used by Stackctl command workflows.

mod service;

use crate::cli::args::{
    DaemonArgs, DaemonCommands, DaemonLogsArgs, DaemonRunArgs, DaemonStartArgs, DaemonStatusArgs,
    DaemonStopArgs, DaemonWatchArgs,
};
use crate::config;
use crate::daemon::{self, DaemonSession};
use crate::output::{self, LogLevel, Persistence};
use anyhow::Result;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(crate) fn handle_daemon(args: &DaemonArgs) -> Result<()> {
    match &args.command {
        DaemonCommands::Start(start) => handle_daemon_start(start),
        DaemonCommands::Watch(watch) => handle_daemon_watch(watch),
        DaemonCommands::Service(service_args) => service::handle_daemon_service(service_args),
        DaemonCommands::Status(status) => handle_daemon_status(status),
        DaemonCommands::Stop(stop) => handle_daemon_stop(stop),
        DaemonCommands::Logs(logs) => handle_daemon_logs(logs),
        DaemonCommands::Run(run) => handle_daemon_run(run),
    }
}

fn handle_daemon_start(args: &DaemonStartArgs) -> Result<()> {
    let project_root = resolve_daemon_project_root(&args.path)?;
    start_daemon_for_project_root(&project_root, true)?;
    Ok(())
}

fn handle_daemon_watch(args: &DaemonWatchArgs) -> Result<()> {
    #[cfg(unix)]
    return handle_daemon_watch_with_runtime_directory(args, &v8_runtime_directory()?);

    #[cfg(not(unix))]
    anyhow::bail!("the v8 singleton daemon requires the Windows named-pipe runtime")
}

#[cfg(unix)]
fn handle_daemon_watch_with_runtime_directory(
    args: &DaemonWatchArgs,
    runtime_directory: &Path,
) -> Result<()> {
    use crate::control_plane::{UnixDaemonWatchOptions, run_unix_daemon_watch};
    let reconciliation = run_unix_daemon_watch(&UnixDaemonWatchOptions {
        runtime_directory: runtime_directory.to_path_buf(),
        watched_roots: args.dir.clone(),
        once: args.once,
        periodic_rescan: Duration::from_secs(args.interval.max(1)),
    })?;
    if args.once {
        if let Some(reconciliation) = reconciliation {
            for issue in reconciliation.report().issues() {
                output::event(
                    "daemon",
                    LogLevel::Error,
                    &issue.to_string(),
                    Persistence::Persistent,
                );
            }
            let status = if reconciliation.was_applied() {
                LogLevel::Success
            } else {
                LogLevel::Error
            };
            output::event(
                "daemon",
                status,
                &format!(
                    "V8 singleton scan found {} project(s) and {} issue(s); registry {}",
                    reconciliation.report().sources().len(),
                    reconciliation.report().issues().len(),
                    if reconciliation.was_applied() {
                        "applied"
                    } else {
                        "preserved"
                    }
                ),
                Persistence::Persistent,
            );
        }
        return Ok(());
    }

    Ok(())
}

#[cfg(unix)]
fn v8_runtime_directory() -> Result<PathBuf> {
    let home = std::env::var("HOME").map_err(|_| anyhow::anyhow!("HOME is not set"))?;
    let home = PathBuf::from(home);

    if cfg!(target_os = "macos") {
        return Ok(home.join("Library/Application Support/stackctl"));
    }
    if let Some(path) = std::env::var_os("XDG_STATE_HOME") {
        return Ok(PathBuf::from(path).join("stackctl"));
    }

    Ok(home.join(".local/state/stackctl"))
}

fn start_daemon_for_project_root(project_root: &Path, log_existing: bool) -> Result<bool> {
    if let Some(existing) = daemon::load_session(&project_root)?
        && daemon::pid_is_running(existing.pid)
    {
        if log_existing {
            output::event(
                "daemon",
                LogLevel::Info,
                &format!(
                    "Daemon already running for {} (pid {})",
                    project_root.display(),
                    existing.pid
                ),
                Persistence::Persistent,
            );
        }
        return Ok(false);
    }

    let log_path = daemon::daemon_log_path(&project_root)?;
    let pid = daemon::spawn_detached(&project_root, &log_path)?;
    daemon::save_session(
        &project_root,
        &DaemonSession {
            project_root: project_root.to_string_lossy().into_owned(),
            pid,
            log_path: log_path.to_string_lossy().into_owned(),
            started_at_unix: daemon::now_unix(),
        },
    )?;
    output::event(
        "daemon",
        LogLevel::Success,
        &format!(
            "Started daemon for {} (pid {})",
            project_root.display(),
            pid
        ),
        Persistence::Persistent,
    );
    Ok(true)
}

fn handle_daemon_status(args: &DaemonStatusArgs) -> Result<()> {
    let project_root = resolve_daemon_project_root(&args.path)?;
    let Some(session) = daemon::load_session(&project_root)? else {
        output::event(
            "daemon",
            LogLevel::Info,
            &format!("No daemon session recorded for {}", project_root.display()),
            Persistence::Persistent,
        );
        return Ok(());
    };
    let message = if daemon::pid_is_running(session.pid) {
        format!(
            "Daemon running for {} (pid {})",
            project_root.display(),
            session.pid
        )
    } else {
        format!(
            "Daemon session for {} is stale (pid {})",
            project_root.display(),
            session.pid
        )
    };
    output::event("daemon", LogLevel::Info, &message, Persistence::Persistent);
    Ok(())
}

fn handle_daemon_stop(args: &DaemonStopArgs) -> Result<()> {
    let project_root = resolve_daemon_project_root(&args.path)?;
    let Some(session) = daemon::load_session(&project_root)? else {
        output::event(
            "daemon",
            LogLevel::Info,
            &format!("No daemon session running for {}", project_root.display()),
            Persistence::Persistent,
        );
        return Ok(());
    };
    daemon::stop_pid(session.pid)?;
    daemon::clear_session(&project_root)?;
    output::event(
        "daemon",
        LogLevel::Success,
        &format!(
            "Stopped daemon for {} (pid {})",
            project_root.display(),
            session.pid
        ),
        Persistence::Persistent,
    );
    Ok(())
}

fn handle_daemon_logs(args: &DaemonLogsArgs) -> Result<()> {
    let project_root = resolve_daemon_project_root(&args.path)?;
    let Some(session) = daemon::load_session(&project_root)? else {
        output::event(
            "daemon",
            LogLevel::Info,
            &format!("No daemon logs found for {}", project_root.display()),
            Persistence::Persistent,
        );
        return Ok(());
    };
    let content = std::fs::read(&session.log_path)?;
    std::io::stdout().write_all(&content)?;
    std::io::stdout().flush()?;
    Ok(())
}

fn handle_daemon_run(args: &DaemonRunArgs) -> Result<()> {
    let project_root = resolve_daemon_project_root(&args.path)?;
    daemon::run_supervisor(&project_root)
}

fn resolve_daemon_project_root(path: &Path) -> Result<PathBuf> {
    config::project_root_with(config::ProjectRootPathOptions::new(None, Some(path)))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::cli::args::{
        DaemonArgs, DaemonCommands, DaemonStartArgs, DaemonStatusArgs, DaemonStopArgs,
        DaemonWatchArgs,
    };

    fn temp_project_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "stackctl-daemon-handler-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("create project root");
        fs::write(
            root.join(".stackctl.toml"),
            "schema_version = 1\nproject_type = \"project\"\nservice = []\nswarm = []\n",
        )
        .expect("write stackctl config");
        root
    }

    fn temp_home(name: &str) -> PathBuf {
        let home = std::env::temp_dir().join(format!(
            "stackctl-daemon-home-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        drop(fs::remove_dir_all(&home));
        fs::create_dir_all(&home).expect("create temporary home");
        home
    }

    fn mock_daemon_binary(home: &PathBuf) -> String {
        let script = home.join("stackctl-daemon-mock");
        fs::write(&script, "#!/usr/bin/env sh\nexec sleep 60\n").expect("write mock daemon");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script)
                .expect("script metadata")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script, perms).expect("chmod mock daemon");
        }
        script.to_string_lossy().to_string()
    }

    #[test]
    fn daemon_start_persists_session_and_stop_clears_it() {
        let project_root = temp_project_root("start-stop");
        let home = temp_home("start-stop");
        let binary = mock_daemon_binary(&home);

        crate::daemon::set_test_daemon_home(home.to_str().expect("home path"));
        crate::daemon::set_test_daemon_binary(&binary);

        super::handle_daemon(&DaemonArgs {
            command: DaemonCommands::Start(DaemonStartArgs {
                path: project_root.clone(),
            }),
        })
        .expect("start daemon");

        let session = crate::daemon::load_session(&project_root)
            .expect("load daemon session")
            .expect("daemon session should exist");
        assert!(crate::daemon::pid_is_running(session.pid));

        super::handle_daemon(&DaemonArgs {
            command: DaemonCommands::Status(DaemonStatusArgs {
                path: project_root.clone(),
            }),
        })
        .expect("status daemon");

        super::handle_daemon(&DaemonArgs {
            command: DaemonCommands::Stop(DaemonStopArgs {
                path: project_root.clone(),
            }),
        })
        .expect("stop daemon");

        assert!(
            crate::daemon::load_session(&project_root)
                .expect("load daemon session after stop")
                .is_none()
        );

        crate::daemon::clear_test_daemon_binary();
        crate::daemon::clear_test_daemon_home();
    }

    #[test]
    fn daemon_watch_once_reconciles_into_one_v8_state_database() {
        let watch_root = temp_home("watch-root");
        let project_root = watch_root.join("project-a");
        fs::create_dir_all(&project_root).expect("create project root");
        fs::write(
            project_root.join(".stackctl.yaml"),
            "schema_version: 8\nproject: project-a\nservices:\n  app:\n    preset: laravel\n",
        )
        .expect("write stackctl config");

        let runtime_directory = std::env::temp_dir().join(format!(
            "s8w-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));

        super::handle_daemon_watch_with_runtime_directory(
            &DaemonWatchArgs {
                dir: vec![watch_root.clone()],
                once: true,
                interval: 1,
            },
            &runtime_directory,
        )
        .expect("watch once");

        let connection = rusqlite::Connection::open(runtime_directory.join("state.sqlite3"))
            .expect("open v8 state");
        let project_name = connection
            .query_row("SELECT project_name FROM projects", [], |row| {
                row.get::<_, String>(0)
            })
            .expect("load v8 project");
        assert_eq!(project_name, "project-a");

        drop(connection);
        fs::remove_dir_all(watch_root).expect("remove watch root");
        fs::remove_dir_all(runtime_directory).expect("remove runtime directory");
    }
}
