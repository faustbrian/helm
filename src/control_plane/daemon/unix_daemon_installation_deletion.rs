use super::{EngineConnectionOutcome, UnixDaemonRuntime, finalize_installation_deletion};
use crate::control_plane::state::InstallationLifecycle;
use std::time::Instant;

impl UnixDaemonRuntime {
    /// Removes the Engine plane only after serialized logical teardown is empty.
    pub(super) fn drive_installation_deletion(&mut self, now: Instant, now_unix_seconds: i64) {
        match self.control_plane.installation_lifecycle() {
            Ok(Some(InstallationLifecycle::Deleting)) => {}
            Ok(_) => return,
            Err(error) => {
                tracing::error!(error = %error, "installation deletion lifecycle could not be read");

                return;
            }
        }
        if self.has_active_project_command()
            || self.has_active_project_backup()
            || self.has_active_project_restore()
            || self.has_active_postgres_prune()
            || self.has_active_migration_decision()
        {
            return;
        }
        match self
            .engine_runtime
            .block_on(self.engine_connection.poll(now))
        {
            EngineConnectionOutcome::Unavailable { retry, detail } => {
                tracing::debug!(
                    attempt = retry.attempt(),
                    retry_milliseconds = retry.duration().as_millis(),
                    error = %detail,
                    "installation deletion is waiting for the selected Engine"
                );

                return;
            }
            EngineConnectionOutcome::Connected | EngineConnectionOutcome::BackingOff { .. } => {}
        }
        let Some(mut engine) = self.engine_connection.engine().cloned() else {
            return;
        };
        let result = self.engine_runtime.block_on(finalize_installation_deletion(
            &mut self.control_plane,
            &mut engine,
            self.global_network_request.metadata().schema_version(),
            now_unix_seconds,
        ));
        match result {
            Ok(true) => tracing::info!("installation deletion reached terminal state"),
            Ok(false) => {}
            Err(error) => {
                tracing::error!(
                    error,
                    "installation deletion could not remove its Engine plane"
                );
            }
        }
    }
}
