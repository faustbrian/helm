use super::{
    ActivePostgresPrune, EngineConnectionOutcome, PostgresPruneExecutionOptions, UnixDaemonRuntime,
    execute_queued_postgres_prune, publish_postgres_prune_result,
    queue_next_installation_deletion_prune,
};
use crate::control_plane::state::{DaemonOperationStatus, DaemonOperationTransitionOptions};
use std::time::{Duration, Instant};

const POSTGRES_PRUNE_TIMEOUT: Duration = Duration::from_secs(2 * 60);

impl UnixDaemonRuntime {
    pub(super) const fn has_active_postgres_prune(&self) -> bool {
        self.active_postgres_prune.is_some()
    }

    /// Advances one idempotent destructive task without overlapping Engine mutation.
    pub(super) fn drive_postgres_prunes(&mut self, now: Instant, now_unix_seconds: i64) {
        self.engine_runtime.block_on(tokio::task::yield_now());
        self.publish_finished_postgres_prune(now_unix_seconds);
        if self.active_postgres_prune.is_some()
            || self.active_project_command.is_some()
            || self.active_project_backup.is_some()
            || self.active_project_restore.is_some()
            || self.active_migration_decision.is_some()
        {
            return;
        }
        match queue_next_installation_deletion_prune(
            &mut self.control_plane,
            &mut self.postgres_prunes,
            &mut self.event_journal,
            now_unix_seconds,
        ) {
            Ok(Some(operation_id)) => {
                tracing::info!(
                    operation_id,
                    "installation deletion queued its next logical prune"
                );
            }
            Ok(None) => {}
            Err(error) => {
                tracing::error!(
                    error,
                    "installation deletion could not queue its next logical prune"
                );

                return;
            }
        }
        match self
            .engine_runtime
            .block_on(self.engine_connection.poll(now))
        {
            EngineConnectionOutcome::Unavailable { retry, detail } => {
                if self.postgres_prunes.len() > 0 {
                    tracing::debug!(
                        attempt = retry.attempt(),
                        retry_milliseconds = retry.duration().as_millis(),
                        error = %detail,
                        "PostgreSQL prune is waiting for the selected Engine"
                    );
                }

                return;
            }
            EngineConnectionOutcome::Connected | EngineConnectionOutcome::BackingOff { .. } => {}
        }
        let Some(engine) = self.engine_connection.engine().cloned() else {
            return;
        };
        let Some(operation) = self.postgres_prunes.pop_front() else {
            return;
        };
        let operation_id = operation.operation_id().to_owned();
        if let Err(error) =
            self.control_plane
                .transition_daemon_operation(DaemonOperationTransitionOptions {
                    operation_id: &operation_id,
                    expected: DaemonOperationStatus::Queued,
                    next: DaemonOperationStatus::Running,
                    updated_at_unix_seconds: now_unix_seconds,
                    event_kind_json: None,
                    event_retention_limit: self.event_journal.capacity(),
                })
        {
            tracing::error!(operation_id, error = %error, "PostgreSQL prune could not claim durable work");
            self.postgres_prunes.requeue_front(operation);

            return;
        }
        let task = self.engine_runtime.spawn(execute_queued_postgres_prune(
            engine,
            PostgresPruneExecutionOptions {
                operation,
                state_database_path: self.options.state_database_path.clone(),
                installation_id: self
                    .global_network_request
                    .metadata()
                    .installation_id()
                    .to_owned(),
                schema_version: self.global_network_request.metadata().schema_version(),
                verified_at_unix_seconds: now_unix_seconds,
                timeout: POSTGRES_PRUNE_TIMEOUT,
            },
        ));
        self.active_postgres_prune = Some(ActivePostgresPrune::new(operation_id, task));
        self.engine_runtime.block_on(tokio::task::yield_now());
    }

    pub(super) fn publish_finished_postgres_prune(&mut self, now_unix_seconds: i64) {
        if !self
            .active_postgres_prune
            .as_ref()
            .is_some_and(ActivePostgresPrune::is_finished)
        {
            return;
        }
        let Some(active) = self.active_postgres_prune.take() else {
            return;
        };
        let (operation_id, task) = active.into_parts();
        match self.engine_runtime.block_on(task) {
            Ok(result) => {
                if let Err(error) = publish_postgres_prune_result(
                    &mut self.control_plane,
                    &mut self.event_journal,
                    result,
                    now_unix_seconds,
                ) {
                    tracing::error!(operation_id, error, "PostgreSQL prune persistence failed");
                }
            }
            Err(error) => {
                let kind_json = super::failed_event_json(
                    "postgres_prune_task_failed",
                    &error.to_string(),
                    &operation_id,
                );
                if let Err(persistence_error) = self.control_plane.transition_daemon_operation(
                    DaemonOperationTransitionOptions {
                        operation_id: &operation_id,
                        expected: DaemonOperationStatus::Running,
                        next: DaemonOperationStatus::Failed,
                        updated_at_unix_seconds: now_unix_seconds,
                        event_kind_json: Some(&kind_json),
                        event_retention_limit: self.event_journal.capacity(),
                    },
                ) {
                    tracing::error!(operation_id, error = %persistence_error, "PostgreSQL prune task failure persistence failed");
                }
            }
        }
    }
}
