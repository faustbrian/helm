use super::{UnixDaemonRuntime, execute_scheduled_project_command};

impl UnixDaemonRuntime {
    pub(super) fn has_active_scheduled_commands(&self) -> bool {
        !self.active_scheduled_commands.is_empty()
    }

    /// Advances and dispatches daemon-owned schedules without blocking IPC.
    pub(super) fn drive_scheduled_project_commands(&mut self, now_unix_seconds: i64) {
        self.engine_runtime.block_on(tokio::task::yield_now());
        self.publish_finished_scheduled_commands();
        if self.has_active_project_command()
            || self.has_active_project_backup()
            || self.has_active_project_restore()
            || self.has_active_postgres_prune()
            || self.has_active_migration_decision()
            || !self.engine_reconciliation.is_converged()
        {
            return;
        }
        let Some(engine) = self.engine_connection.engine().cloned() else {
            return;
        };
        if !self.scheduled_command_clock.take_due(now_unix_seconds) {
            return;
        }

        let installation_id = self
            .global_network_request
            .metadata()
            .installation_id()
            .to_owned();
        let schema_version = self.global_network_request.metadata().schema_version();
        for plan in self.scheduled_project_commands.clone() {
            let key = (plan.project_id().to_owned(), plan.service_id().to_owned());
            if self.active_scheduled_commands.contains_key(&key) {
                tracing::warn!(
                    project = key.0,
                    service = key.1,
                    "scheduled project command skipped because its prior run is still active"
                );
                continue;
            }
            let task = self.engine_runtime.spawn(execute_scheduled_project_command(
                engine.clone(),
                plan,
                installation_id.clone(),
                schema_version,
            ));
            self.active_scheduled_commands.insert(key, task);
        }
        self.engine_runtime.block_on(tokio::task::yield_now());
    }

    pub(super) fn publish_finished_scheduled_commands(&mut self) {
        let finished = self
            .active_scheduled_commands
            .iter()
            .filter_map(|(key, task)| task.is_finished().then_some(key.clone()))
            .collect::<Vec<_>>();
        for key in finished {
            let Some(task) = self.active_scheduled_commands.remove(&key) else {
                continue;
            };
            match self.engine_runtime.block_on(task) {
                Ok(Ok(output)) => tracing::debug!(
                    project = key.0,
                    service = key.1,
                    stdout_bytes = output.stdout().len(),
                    stderr_bytes = output.stderr().len(),
                    "scheduled project command completed"
                ),
                Ok(Err(error)) => {
                    self.engine_reconciliation.request();
                    tracing::error!(
                        project = key.0,
                        service = key.1,
                        error = %error,
                        "scheduled project command failed"
                    );
                }
                Err(error) => {
                    self.engine_reconciliation.request();
                    tracing::error!(
                        project = key.0,
                        service = key.1,
                        error = %error,
                        "scheduled project command task failed"
                    );
                }
            }
        }
    }
}
