use super::ipc::IpcEventKind;
use super::{
    ActiveProjectCommand, EngineConnectionOutcome, UnixDaemonRuntime,
    execute_queued_project_command, publish_project_command_result,
};
use crate::control_plane::state::{DaemonOperationStatus, DaemonOperationTransitionOptions};
use std::time::Instant;

impl UnixDaemonRuntime {
    /// Reports whether Engine topology mutation must remain serialized.
    pub(super) const fn has_active_project_command(&self) -> bool {
        self.active_project_command.is_some()
    }

    /// Advances one bounded background command without extending IPC handling.
    pub(super) fn drive_project_commands(&mut self, now: Instant, now_unix_seconds: i64) {
        self.engine_runtime.block_on(tokio::task::yield_now());
        self.publish_finished_project_command(now_unix_seconds);
        if self.active_project_command.is_some() {
            return;
        }

        match self
            .engine_runtime
            .block_on(self.engine_connection.poll(now))
        {
            EngineConnectionOutcome::Unavailable { retry, detail } => {
                if self.project_commands.len() > 0 {
                    tracing::debug!(
                        attempt = retry.attempt(),
                        retry_milliseconds = retry.duration().as_millis(),
                        error = %detail,
                        "project command is waiting for the selected Engine"
                    );
                }

                return;
            }
            EngineConnectionOutcome::Connected | EngineConnectionOutcome::BackingOff { .. } => {}
        }

        let Some(engine) = self.engine_connection.engine().cloned() else {
            return;
        };
        let Some(operation_id) = self
            .project_commands
            .front()
            .map(|operation| operation.operation_id().to_owned())
        else {
            return;
        };
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
            tracing::error!(
                operation_id,
                error = %error,
                "project command could not claim its durable queue entry"
            );

            return;
        }
        let operation = self
            .project_commands
            .pop_front()
            .expect("durably claimed project command remains queued");
        let installation_id = self
            .global_network_request
            .metadata()
            .installation_id()
            .to_owned();
        let schema_version = self.global_network_request.metadata().schema_version();
        let task = self.engine_runtime.spawn(execute_queued_project_command(
            engine,
            operation,
            installation_id,
            schema_version,
        ));
        self.active_project_command = Some(ActiveProjectCommand::new(operation_id, task));
        self.engine_runtime.block_on(tokio::task::yield_now());
    }

    fn publish_finished_project_command(&mut self, now_unix_seconds: i64) {
        if !self
            .active_project_command
            .as_ref()
            .is_some_and(ActiveProjectCommand::is_finished)
        {
            return;
        }
        let active = self
            .active_project_command
            .take()
            .expect("finished project command was present");
        let (operation_id, task) = active.into_parts();
        match self.engine_runtime.block_on(task) {
            Ok(result) => {
                if let Err(error) = publish_project_command_result(
                    &mut self.control_plane,
                    &mut self.event_journal,
                    result,
                    now_unix_seconds,
                ) {
                    tracing::error!(
                        operation_id,
                        error,
                        "project command result persistence failed"
                    );
                }
            }
            Err(error) => {
                let event = IpcEventKind::Failed {
                    code: "project_command_task_failed".to_owned(),
                    message: error.to_string(),
                };
                let kind_json = serde_json::to_string(&event)
                    .expect("project command task failure serialization is infallible");
                let persistence = self.control_plane.transition_daemon_operation(
                    DaemonOperationTransitionOptions {
                        operation_id: &operation_id,
                        expected: DaemonOperationStatus::Running,
                        next: DaemonOperationStatus::Failed,
                        updated_at_unix_seconds: now_unix_seconds,
                        event_kind_json: Some(&kind_json),
                        event_retention_limit: self.event_journal.capacity(),
                    },
                );
                let persistence =
                    persistence
                        .map_err(|error| error.to_string())
                        .and_then(|event| {
                            let event = event.ok_or_else(|| {
                                "task failure transition omitted its event".to_owned()
                            })?;
                            self.event_journal
                                .append_record(event)
                                .map_err(|error| error.to_string())
                        });
                if let Err(persistence_error) = persistence {
                    tracing::error!(
                        operation_id,
                        error = persistence_error,
                        "project command task failure persistence failed"
                    );
                }
            }
        }
    }
}
