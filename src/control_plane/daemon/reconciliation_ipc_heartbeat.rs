use super::{
    DaemonRequestDispatchOptions, DiscoveryScheduler, IpcEventJournal, MigrationDecisionQueue,
    PostgresPruneQueue, ProjectBackupQueue, ProjectCommandQueue, ProjectDiscoveryOptions,
    ProjectLogSessionRegistry, ProjectRestoreQueue, ResourceHealthRegistry,
    dispatch_daemon_request, requires_engine_reconciliation, requires_followup_reconciliation,
};
use crate::control_plane::application::ControlPlane;
use crate::control_plane::daemon::ipc::{IpcDiagnostic, UnixIpcListener};
use crate::control_plane::state::StateStore;
use std::time::Instant;

/// Mutable daemon boundaries that remain available while Engine tasks run.
pub(crate) struct ReconciliationIpcHeartbeat<'operation, Store> {
    options: ReconciliationIpcHeartbeatOptions<'operation, Store>,
    error: Option<String>,
}

pub(crate) struct ReconciliationIpcHeartbeatOptions<'operation, Store> {
    pub(crate) listener: &'operation UnixIpcListener,
    pub(crate) control_plane: &'operation mut ControlPlane<Store>,
    pub(crate) discovery_options: ProjectDiscoveryOptions,
    pub(crate) event_journal: &'operation mut IpcEventJournal,
    pub(crate) project_commands: &'operation mut ProjectCommandQueue,
    pub(crate) project_backups: &'operation mut ProjectBackupQueue,
    pub(crate) postgres_prunes: &'operation mut PostgresPruneQueue,
    pub(crate) project_restores: &'operation mut ProjectRestoreQueue,
    pub(crate) migration_decisions: &'operation mut MigrationDecisionQueue,
    pub(crate) project_logs: &'operation mut ProjectLogSessionRegistry,
    pub(crate) resource_health: &'operation ResourceHealthRegistry,
    pub(crate) discovery_diagnostics: &'operation [IpcDiagnostic],
    pub(crate) scheduler: &'operation mut DiscoveryScheduler,
    pub(crate) now: Instant,
    pub(crate) now_unix_seconds: i64,
}

impl<'operation, Store> ReconciliationIpcHeartbeat<'operation, Store>
where
    Store: StateStore,
{
    pub(crate) fn new(options: ReconciliationIpcHeartbeatOptions<'operation, Store>) -> Self {
        Self {
            options,
            error: None,
        }
    }

    /// Serves at most one pending request without crossing the busy Engine
    /// boundary from inside its current runtime task.
    pub(crate) fn serve(&mut self) {
        if self.error.is_some() {
            return;
        }
        let options = &mut self.options;
        let request = options.listener.try_serve_next(|request| {
            dispatch_daemon_request(DaemonRequestDispatchOptions {
                control_plane: options.control_plane,
                discovery_options: options.discovery_options,
                request,
                event_journal: options.event_journal,
                project_commands: options.project_commands,
                project_backups: options.project_backups,
                postgres_prunes: options.postgres_prunes,
                project_restores: options.project_restores,
                migration_decisions: options.migration_decisions,
                project_logs: options.project_logs,
                resource_health: options.resource_health,
                discovery_diagnostics: options.discovery_diagnostics,
                benchmark_snapshot: None,
                image_reference_resolution: None,
                now_unix_seconds: options.now_unix_seconds,
            })
        });
        match request {
            Ok(Some(request)) => {
                if requires_followup_reconciliation(&request)
                    || requires_engine_reconciliation(&request)
                {
                    options.scheduler.record_filesystem_event(options.now);
                }
            }
            Ok(None) => {}
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    pub(crate) fn finish(self) -> Result<(), String> {
        self.error.map_or(Ok(()), Err)
    }
}

#[cfg(test)]
mod tests {
    use super::{ReconciliationIpcHeartbeat, ReconciliationIpcHeartbeatOptions};
    use crate::control_plane::application::ControlPlane;
    use crate::control_plane::daemon::ipc::{
        IpcOutcome, IpcPayload, IpcRequest, IpcResult, UnixIpcListener, send_unix_request,
    };
    use crate::control_plane::daemon::{
        DiscoveryScheduler, DiscoverySchedulerOptions, IpcEventJournal, MigrationDecisionQueue,
        PostgresPruneQueue, ProjectBackupQueue, ProjectCommandQueue, ProjectDiscoveryOptions,
        ProjectLogSessionRegistry, ProjectRestoreQueue, ResourceHealthRegistry,
    };
    use crate::control_plane::state::SqliteStateStore;
    use std::sync::{Arc, Barrier};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    #[test]
    fn reconciliation_heartbeat_answers_ping_during_engine_work() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "stackctl-reconciliation-heartbeat-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("heartbeat fixture");
        let socket_path = std::env::temp_dir().join(format!(
            "s8h-{}-{}.sock",
            std::process::id(),
            unique % 1_000_000_000
        ));
        let listener = UnixIpcListener::bind(&socket_path).expect("heartbeat listener");
        listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let barrier = Arc::new(Barrier::new(2));
        let client_barrier = Arc::clone(&barrier);
        let client_socket = socket_path.clone();
        let client = std::thread::spawn(move || {
            client_barrier.wait();
            send_unix_request(
                &client_socket,
                &IpcRequest::new("heartbeat-ping", IpcPayload::Ping),
                Duration::from_secs(1),
            )
            .expect("heartbeat response")
        });
        let now = Instant::now();
        let mut control_plane = ControlPlane::new(
            SqliteStateStore::open(&root.join("state.sqlite3")).expect("heartbeat state"),
        );
        let mut event_journal = IpcEventJournal::default();
        let mut project_commands = ProjectCommandQueue::default();
        let mut project_backups = ProjectBackupQueue::default();
        let mut postgres_prunes = PostgresPruneQueue::default();
        let mut project_restores = ProjectRestoreQueue::default();
        let mut migration_decisions = MigrationDecisionQueue::default();
        let mut project_logs = ProjectLogSessionRegistry::default();
        let resource_health = ResourceHealthRegistry::default();
        let mut scheduler = DiscoveryScheduler::new(
            now,
            DiscoverySchedulerOptions::new(
                Duration::from_millis(10),
                Duration::from_millis(20),
                Duration::from_secs(60),
            )
            .expect("scheduler options"),
        );
        let mut heartbeat = ReconciliationIpcHeartbeat::new(ReconciliationIpcHeartbeatOptions {
            listener: &listener,
            control_plane: &mut control_plane,
            discovery_options: ProjectDiscoveryOptions::bounded_defaults(),
            event_journal: &mut event_journal,
            project_commands: &mut project_commands,
            project_backups: &mut project_backups,
            postgres_prunes: &mut postgres_prunes,
            project_restores: &mut project_restores,
            migration_decisions: &mut migration_decisions,
            project_logs: &mut project_logs,
            resource_health: &resource_health,
            discovery_diagnostics: &[],
            scheduler: &mut scheduler,
            now,
            now_unix_seconds: 1,
        });
        barrier.wait();
        for _ in 0..1_000 {
            heartbeat.serve();
            if client.is_finished() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }

        heartbeat.finish().expect("healthy IPC heartbeat");
        let response = client.join().expect("heartbeat client");
        assert!(matches!(
            response.outcome(),
            IpcOutcome::Success {
                result: IpcResult::Pong
            }
        ));
        drop(listener);
        drop(control_plane);
        drop(std::fs::remove_file(socket_path));
        std::fs::remove_dir_all(root).expect("remove heartbeat fixture");
    }
}
