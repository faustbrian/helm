//! cli handlers daemon cmd module.
//!
//! Contains pre-config daemon command routing used by Stackctl command workflows.

mod activate_gateway_certificate;
mod backup;
mod benchmark;
mod delete_data_uninstall;
mod migration_decision;
mod prune;
mod restore;
mod retained;
mod service;
mod trust;
mod validate_benchmark_project_count;
mod validate_benchmark_scenario;

use crate::cli::args::{
    DaemonAdoptArgs, DaemonArgs, DaemonBackupsArgs, DaemonCommands, DaemonMigrationArgs,
    DaemonMigrationCommands, DaemonMigrationStatusArgs, DaemonWatchArgs,
};
use crate::output::{self, LogLevel, Persistence};
use anyhow::Result;
use std::path::Path;
use std::time::Duration;

pub(crate) fn handle_daemon(args: &DaemonArgs) -> Result<()> {
    match &args.command {
        DaemonCommands::Watch(watch) => handle_daemon_watch(watch),
        DaemonCommands::Service(service_args) => service::handle_daemon_service(service_args),
        DaemonCommands::Status => handle_daemon_status(),
        DaemonCommands::Retained(retained_args) => retained::handle_daemon_retained(retained_args),
        DaemonCommands::Reconcile => handle_daemon_reconcile(),
        DaemonCommands::Benchmark(benchmark_args) => {
            benchmark::handle_daemon_benchmark(benchmark_args)
        }
        DaemonCommands::Adopt(adopt) => handle_daemon_adopt(adopt),
        DaemonCommands::Backup(backup_args) => backup::handle_daemon_backup(backup_args),
        DaemonCommands::Backups(backups_args) => handle_daemon_backups(backups_args),
        DaemonCommands::Restore(restore_args) => restore::handle_daemon_restore(restore_args),
        DaemonCommands::Prune(prune_args) => prune::handle_daemon_prune(prune_args),
        DaemonCommands::Migration(migration) => handle_daemon_migration(migration),
        DaemonCommands::Trust(trust_args) => trust::handle_daemon_trust(trust_args),
    }
}

#[cfg(unix)]
fn handle_daemon_backups(args: &DaemonBackupsArgs) -> Result<()> {
    use crate::control_plane::{IpcOutcome, IpcPayload, IpcResult};

    let canonical_path = std::fs::canonicalize(&args.path)?;
    let response = send_singleton_request(IpcPayload::ProjectRecoveryPoints { canonical_path })?;
    match response.outcome() {
        IpcOutcome::Success {
            result: IpcResult::ProjectRecoveryPoints { recovery_points },
        } => {
            if recovery_points.is_empty() {
                output::event(
                    "daemon",
                    LogLevel::Info,
                    "No verified recovery points exist for this project",
                    Persistence::Persistent,
                );
            }
            for point in recovery_points {
                output::event(
                    "daemon",
                    LogLevel::Info,
                    &format!(
                        "{}: service={}, recovery_point={}, bytes={}, sha256={}, created_at={}, verified_at={}",
                        point.recovery_point_id(),
                        point.service(),
                        point.recovery_point(),
                        point.artifact_size_bytes(),
                        point.artifact_sha256(),
                        point.created_at_unix_seconds(),
                        point.verified_at_unix_seconds(),
                    ),
                    Persistence::Persistent,
                );
            }

            Ok(())
        }
        IpcOutcome::Failure { diagnostics } => {
            let diagnostic = diagnostics
                .iter()
                .map(|item| format!("{}: {}", item.code(), item.message()))
                .collect::<Vec<_>>()
                .join("; ");
            anyhow::bail!("recovery-point listing failed: {diagnostic}")
        }
        outcome => anyhow::bail!("unexpected recovery-point response: {outcome:?}"),
    }
}

fn handle_daemon_migration(args: &DaemonMigrationArgs) -> Result<()> {
    match &args.command {
        DaemonMigrationCommands::Status(status) => handle_daemon_migration_status(status),
        DaemonMigrationCommands::Confirm(decision) => {
            migration_decision::handle_migration_decision(
                decision,
                crate::control_plane::IpcMigrationDecision::Confirm,
            )
        }
        DaemonMigrationCommands::Rollback(decision) => {
            migration_decision::handle_migration_decision(
                decision,
                crate::control_plane::IpcMigrationDecision::Rollback,
            )
        }
    }
}

#[cfg(unix)]
fn handle_daemon_migration_status(args: &DaemonMigrationStatusArgs) -> Result<()> {
    use crate::control_plane::{IpcOutcome, IpcPayload, IpcResult};

    let canonical_path = std::fs::canonicalize(&args.path)?;
    let response = send_singleton_request(IpcPayload::ProjectMigrations { canonical_path })?;
    match response.outcome() {
        IpcOutcome::Success {
            result: IpcResult::ProjectMigrations { migrations },
        } => {
            if migrations.is_empty() {
                output::event(
                    "daemon",
                    LogLevel::Info,
                    "No durable migrations exist for this project",
                    Persistence::Persistent,
                );
            }
            for migration in migrations {
                output::event(
                    "daemon",
                    LogLevel::Info,
                    &format!(
                        "{}: phase={}, backup_verified={}, awaiting_confirmation={}, updated_at={}",
                        migration.migration_id(),
                        migration.phase(),
                        migration.backup_verified(),
                        migration.awaiting_confirmation(),
                        migration.updated_at_unix_seconds(),
                    ),
                    Persistence::Persistent,
                );
            }
            Ok(())
        }
        IpcOutcome::Failure { diagnostics } => {
            let diagnostic = diagnostics
                .iter()
                .map(|item| format!("{}: {}", item.code(), item.message()))
                .collect::<Vec<_>>()
                .join("; ");
            anyhow::bail!("migration status failed: {diagnostic}")
        }
        outcome => anyhow::bail!("unexpected migration status response: {outcome:?}"),
    }
}

#[cfg(unix)]
fn handle_daemon_adopt(args: &DaemonAdoptArgs) -> Result<()> {
    use crate::control_plane::{IpcOutcome, IpcPayload, IpcResult};

    let canonical_path = std::fs::canonicalize(&args.path)?;
    let response = send_singleton_request(IpcPayload::AdoptProject { canonical_path })?;
    match response.outcome() {
        IpcOutcome::Success {
            result: IpcResult::ProjectAdopted { project_id },
        } => {
            output::event(
                "daemon",
                LogLevel::Success,
                &format!("Adopted retained state for project '{project_id}'"),
                Persistence::Persistent,
            );
            Ok(())
        }
        IpcOutcome::Failure { diagnostics } => {
            let diagnostic = diagnostics
                .iter()
                .map(|item| format!("{}: {}", item.code(), item.message()))
                .collect::<Vec<_>>()
                .join("; ");
            anyhow::bail!("project adoption failed: {diagnostic}")
        }
        outcome => anyhow::bail!("unexpected project adoption response: {outcome:?}"),
    }
}

fn handle_daemon_watch(args: &DaemonWatchArgs) -> Result<()> {
    handle_daemon_watch_with_runtime_directory(
        args,
        &crate::control_plane::default_unix_daemon_runtime_directory()?,
    )
}

#[cfg(unix)]
fn handle_daemon_watch_with_runtime_directory(
    args: &DaemonWatchArgs,
    runtime_directory: &Path,
) -> Result<()> {
    use crate::control_plane::{UnixDaemonWatchOptions, run_unix_daemon_watch};
    let reconciliation = match run_unix_daemon_watch(&UnixDaemonWatchOptions {
        runtime_directory: runtime_directory.to_path_buf(),
        watched_roots: args.dir.clone(),
        once: args.once,
        periodic_rescan: Duration::from_secs(args.interval.max(1)),
    }) {
        Ok(reconciliation) => reconciliation,
        Err(error) => {
            output::event(
                "daemon",
                LogLevel::Error,
                &daemon_watch_failure_message(&error),
                Persistence::Persistent,
            );

            return Err(error.into());
        }
    };
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

fn daemon_watch_failure_message(error: impl std::fmt::Display) -> String {
    format!("V8 singleton daemon stopped before it became ready: {error}")
}

#[cfg(unix)]
fn handle_daemon_status() -> Result<()> {
    use crate::control_plane::{IpcOutcome, IpcPayload, IpcResult};

    let response = send_singleton_request(IpcPayload::DaemonStatus)?;
    match response.outcome() {
        IpcOutcome::Success {
            result:
                IpcResult::DaemonStatus {
                    discovery_complete: true,
                    engine_available: true,
                    engine_converged: true,
                    discovery_diagnostics,
                    reconciliation_diagnostic: None,
                },
        } if discovery_diagnostics.is_empty() => {
            output::event(
                "daemon",
                LogLevel::Success,
                "V8 singleton daemon is responsive",
                Persistence::Persistent,
            );
            Ok(())
        }
        IpcOutcome::Success {
            result:
                IpcResult::DaemonStatus {
                    discovery_complete,
                    engine_available,
                    engine_converged,
                    discovery_diagnostics,
                    reconciliation_diagnostic,
                },
        } => {
            let mut issues = Vec::new();
            if !discovery_complete {
                issues.push(
                    "discovery_incomplete: no complete validated project registry is active"
                        .to_owned(),
                );
            }
            if !engine_available {
                issues.push(
                    "engine_unavailable: selected container Engine is unavailable".to_owned(),
                );
            }
            if !engine_converged {
                issues.push(
                    "engine_not_converged: managed services, gateway, or routes have not fully reconciled"
                        .to_owned(),
                );
            }
            issues.extend(
                discovery_diagnostics
                    .iter()
                    .map(|diagnostic| format!("{}: {}", diagnostic.code(), diagnostic.message())),
            );
            if let Some(diagnostic) = reconciliation_diagnostic {
                issues.push(format!("{}: {}", diagnostic.code(), diagnostic.message()));
            }
            anyhow::bail!("singleton is not ready: {}", issues.join("; "))
        }
        IpcOutcome::Failure { diagnostics } => {
            let detail = diagnostics
                .iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.code(), diagnostic.message()))
                .collect::<Vec<_>>()
                .join("; ");
            anyhow::bail!("singleton status failed: {detail}")
        }
        outcome => anyhow::bail!("unexpected singleton status response: {outcome:?}"),
    }
}

#[cfg(unix)]
fn handle_daemon_reconcile() -> Result<()> {
    use crate::control_plane::{IpcOutcome, IpcPayload, IpcResult};

    let response = send_singleton_request(IpcPayload::Reconcile)?;
    match response.outcome() {
        IpcOutcome::Success {
            result: IpcResult::Reconciled { project_count },
        } => {
            output::event(
                "daemon",
                LogLevel::Success,
                &format!("Singleton reconciliation applied {project_count} project(s)"),
                Persistence::Persistent,
            );
            Ok(())
        }
        IpcOutcome::Failure { diagnostics } => {
            let diagnostic = diagnostics
                .iter()
                .map(|item| {
                    format!(
                        "{}: {}{}",
                        item.code(),
                        item.message(),
                        if item.retryable() { " (retryable)" } else { "" }
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            anyhow::bail!("singleton reconciliation failed: {diagnostic}")
        }
        outcome => anyhow::bail!("unexpected singleton reconciliation response: {outcome:?}"),
    }
}

#[cfg(unix)]
pub(super) fn send_singleton_request(
    payload: crate::control_plane::IpcPayload,
) -> Result<crate::control_plane::IpcResponse> {
    send_singleton_request_with_timeout(payload, Duration::from_secs(5))
}

#[cfg(unix)]
fn send_singleton_request_with_timeout(
    payload: crate::control_plane::IpcPayload,
    timeout: Duration,
) -> Result<crate::control_plane::IpcResponse> {
    use crate::control_plane::{IpcRequest, default_unix_daemon_runtime_directory};
    use std::time::{SystemTime, UNIX_EPOCH};

    let request_id = format!(
        "cli-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    );
    let request = IpcRequest::new(request_id, payload);
    let socket_path = default_unix_daemon_runtime_directory()?.join("daemon.sock");

    crate::control_plane::send_unix_request(&socket_path, &request, timeout).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::net::{IpAddr, Ipv4Addr};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::cli::args::DaemonWatchArgs;
    use crate::control_plane::{GatewayError, LocalhostResolver};

    struct LoopbackResolver;

    impl LocalhostResolver for LoopbackResolver {
        fn resolve(&self, _host: &str) -> Result<Vec<IpAddr>, GatewayError> {
            Ok(vec![IpAddr::V4(Ipv4Addr::LOCALHOST)])
        }
    }

    fn watch_once(args: &DaemonWatchArgs, runtime_directory: &std::path::Path) {
        use crate::control_plane::{UnixDaemonWatchOptions, run_unix_daemon_watch_with_resolver};

        run_unix_daemon_watch_with_resolver(
            &UnixDaemonWatchOptions {
                runtime_directory: runtime_directory.to_path_buf(),
                watched_roots: args.dir.clone(),
                once: args.once,
                periodic_rescan: std::time::Duration::from_secs(args.interval.max(1)),
            },
            &LoopbackResolver,
        )
        .expect("watch once");
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

    #[test]
    fn daemon_watch_failure_message_preserves_actionable_startup_detail() {
        assert_eq!(
            super::daemon_watch_failure_message("resolver unavailable"),
            "V8 singleton daemon stopped before it became ready: resolver unavailable"
        );
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
        fs::write(
            project_root.join(".stackctl.lock.yaml"),
            concat!(
                "schema_version: 1\ncatalog_revision: 2026-07-15.3\nimages:\n",
                "  app:\n    source: preset:laravel\n",
                "    resolved: dunglas/frankenphp@sha256:",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"
            ),
        )
        .expect("write stackctl lock");

        let runtime_directory = std::env::temp_dir().join(format!(
            "s8w-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));

        watch_once(
            &DaemonWatchArgs {
                dir: vec![watch_root.clone()],
                once: true,
                interval: 1,
            },
            &runtime_directory,
        );

        let connection = rusqlite::Connection::open(runtime_directory.join("state.sqlite3"))
            .expect("open v8 state");
        let project_name = connection
            .query_row("SELECT project_name FROM projects", [], |row| {
                row.get::<_, String>(0)
            })
            .expect("load v8 project");
        assert_eq!(project_name, "project-a");
        let installation = connection
            .query_row(
                "SELECT installation_id, engine_provider, engine_endpoint FROM installation",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .expect("load v8 installation");
        assert_eq!(installation.1, "docker");
        assert!(
            !installation.0.is_empty(),
            "installation identity must be persisted"
        );
        assert!(
            !installation.2.is_empty(),
            "Engine endpoint must be persisted"
        );
        drop(connection);

        watch_once(
            &DaemonWatchArgs {
                dir: vec![watch_root.clone()],
                once: true,
                interval: 1,
            },
            &runtime_directory,
        );
        let connection = rusqlite::Connection::open(runtime_directory.join("state.sqlite3"))
            .expect("reopen v8 state");
        let restarted_installation_id = connection
            .query_row("SELECT installation_id FROM installation", [], |row| {
                row.get::<_, String>(0)
            })
            .expect("load restarted v8 installation");
        assert_eq!(restarted_installation_id, installation.0);

        drop(connection);
        fs::remove_dir_all(watch_root).expect("remove watch root");
        fs::remove_dir_all(runtime_directory).expect("remove runtime directory");
    }
}
