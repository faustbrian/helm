//! cli handlers daemon cmd module.
//!
//! Contains pre-config daemon command routing used by Helm command workflows.

use crate::cli::args::{
    DaemonArgs, DaemonCommands, DaemonLogsArgs, DaemonRunArgs, DaemonStartArgs, DaemonStatusArgs,
    DaemonStopArgs,
};
use crate::config;
use crate::daemon::{self, DaemonSession};
use crate::output::{self, LogLevel, Persistence};
use anyhow::Result;
use std::io::Write;
use std::path::{Path, PathBuf};

pub(crate) fn handle_daemon(args: &DaemonArgs) -> Result<()> {
    match &args.command {
        DaemonCommands::Start(start) => handle_daemon_start(start),
        DaemonCommands::Status(status) => handle_daemon_status(status),
        DaemonCommands::Stop(stop) => handle_daemon_stop(stop),
        DaemonCommands::Logs(logs) => handle_daemon_logs(logs),
        DaemonCommands::Run(run) => handle_daemon_run(run),
    }
}

fn handle_daemon_start(args: &DaemonStartArgs) -> Result<()> {
    let project_root = resolve_daemon_project_root(&args.path)?;
    if let Some(existing) = daemon::load_session(&project_root)?
        && daemon::pid_is_running(existing.pid)
    {
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
        return Ok(());
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
    Ok(())
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
    };

    fn temp_project_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "helm-daemon-handler-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("create project root");
        fs::write(
            root.join(".helm.toml"),
            "schema_version = 1\nproject_type = \"project\"\nservice = []\nswarm = []\n",
        )
        .expect("write helm config");
        root
    }

    fn temp_home(name: &str) -> PathBuf {
        let home = std::env::temp_dir().join(format!(
            "helm-daemon-home-{name}-{}",
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
        let script = home.join("helm-daemon-mock");
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
}
