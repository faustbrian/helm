use super::{UnixDaemonRuntime, unix_daemon_runtime::unix_time_seconds};
use std::time::Duration;

const SHUTDOWN_DRAIN_POLL_INTERVAL: Duration = Duration::from_millis(10);

impl UnixDaemonRuntime {
    /// Finishes already-running mutation tasks without claiming queued work.
    pub(super) fn drain_active_operations_for_shutdown(&mut self) {
        self.stop_project_logs_for_shutdown();
        while self.has_active_mutation() {
            self.engine_runtime.block_on(tokio::task::yield_now());
            let now_unix_seconds = unix_time_seconds();
            self.publish_finished_project_command(now_unix_seconds);
            self.publish_finished_project_backup(now_unix_seconds);
            self.publish_finished_postgres_prune(now_unix_seconds);
            self.publish_finished_project_restore(now_unix_seconds);
            self.publish_finished_migration_decision(now_unix_seconds);
            self.publish_finished_scheduled_commands();
            if self.has_active_mutation() {
                std::thread::sleep(SHUTDOWN_DRAIN_POLL_INTERVAL);
            }
        }
    }

    fn has_active_mutation(&self) -> bool {
        self.has_active_project_command()
            || self.has_active_project_backup()
            || self.has_active_postgres_prune()
            || self.has_active_project_restore()
            || self.has_active_migration_decision()
            || self.has_active_scheduled_commands()
    }
}
