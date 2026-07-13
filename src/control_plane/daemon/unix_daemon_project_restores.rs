use super::ipc::IpcEventKind;
use super::{
    ActiveProjectRestore, EngineConnectionOutcome, ProjectRestoreExecutionOptions,
    UnixDaemonRuntime, execute_queued_project_restore, publish_project_restore_result,
};
use crate::control_plane::shared_infrastructure::{
    OsCredentialEntropy, SharedInstancePlan, resolve_execution_shared_instances,
};
use crate::control_plane::state::{DaemonOperationStatus, DaemonOperationTransitionOptions};
use std::time::{Duration, Instant};

const PROJECT_RESTORE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

impl UnixDaemonRuntime {
    /// Reports whether a retained restore is mutating Engine and durable state.
    pub(super) const fn has_active_project_restore(&self) -> bool {
        self.active_project_restore.is_some()
    }

    /// Advances one exact recovery point without overlapping Engine mutation.
    pub(super) fn drive_project_restores(&mut self, now: Instant, now_unix_seconds: i64) {
        self.engine_runtime.block_on(tokio::task::yield_now());
        self.publish_finished_project_restore(now_unix_seconds);
        if self.active_project_restore.is_some()
            || self.active_project_command.is_some()
            || self.active_project_backup.is_some()
        {
            return;
        }

        match self
            .engine_runtime
            .block_on(self.engine_connection.poll(now))
        {
            EngineConnectionOutcome::Unavailable { retry, detail } => {
                if self.project_restores.len() > 0 {
                    tracing::debug!(
                        attempt = retry.attempt(),
                        retry_milliseconds = retry.duration().as_millis(),
                        error = %detail,
                        "project restore is waiting for the selected Engine"
                    );
                }

                return;
            }
            EngineConnectionOutcome::Connected | EngineConnectionOutcome::BackingOff { .. } => {}
        }

        let Some(engine) = self.engine_connection.engine().cloned() else {
            return;
        };
        let Some(operation) = self.project_restores.front() else {
            return;
        };
        let Some(shared) = self.project_restore_shared_plan(operation) else {
            return;
        };
        let operation_id = operation.operation_id().to_owned();
        let shared = match shared {
            Ok(shared) => shared,
            Err(message) => {
                self.reject_queued_project_restore(&operation_id, &message, now_unix_seconds);

                return;
            }
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
                "project restore could not claim its durable queue entry"
            );

            return;
        }
        let operation = self
            .project_restores
            .pop_front()
            .expect("durably claimed project restore remains queued");
        let task = self.engine_runtime.spawn(execute_queued_project_restore(
            engine,
            OsCredentialEntropy,
            ProjectRestoreExecutionOptions {
                operation,
                shared,
                installation_id: self
                    .global_network_request
                    .metadata()
                    .installation_id()
                    .to_owned(),
                network_name: self.global_network_request.name().to_owned(),
                schema_version: self.global_network_request.metadata().schema_version(),
                state_database_path: self.options.state_database_path.clone(),
                backup_root: self.runtime_directory.join("backups"),
                updated_at_unix_seconds: now_unix_seconds,
                timeout: PROJECT_RESTORE_TIMEOUT,
            },
        ));
        self.active_project_restore = Some(ActiveProjectRestore::new(operation_id, task));
        self.engine_runtime.block_on(tokio::task::yield_now());
    }

    fn project_restore_shared_plan(
        &self,
        operation: &super::QueuedProjectRestore,
    ) -> Option<Result<SharedInstancePlan, String>> {
        let execution = self.engine_reconciliation.execution_plan()?;
        let platform = match super::unix_daemon_runtime::runtime_linux_platform() {
            Ok(platform) => platform,
            Err(error) => return Some(Err(error)),
        };
        let shared = match resolve_execution_shared_instances(execution, platform) {
            Ok(shared) => shared,
            Err(error) => return Some(Err(error.to_string())),
        };
        let matches = shared
            .into_iter()
            .filter(|plan| {
                plan.profile().implementation() == "postgresql"
                    && plan.fingerprint().as_str() == operation.compatibility_fingerprint()
            })
            .collect::<Vec<_>>();
        Some(match matches.as_slice() {
            [shared] => Ok(shared.clone()),
            [] => Err(format!(
                "recovery point '{}' has no exact PostgreSQL compatibility plan",
                operation.recovery_point_id()
            )),
            _ => Err(format!(
                "recovery point '{}' matched multiple PostgreSQL compatibility plans",
                operation.recovery_point_id()
            )),
        })
    }

    fn reject_queued_project_restore(
        &mut self,
        operation_id: &str,
        message: &str,
        now_unix_seconds: i64,
    ) {
        let event = IpcEventKind::Failed {
            code: "project_restore_plan_invalid".to_owned(),
            message: message.to_owned(),
        };
        let kind_json = serde_json::to_string(&event)
            .expect("project restore plan failure serialization is infallible");
        match self
            .control_plane
            .transition_daemon_operation(DaemonOperationTransitionOptions {
                operation_id,
                expected: DaemonOperationStatus::Queued,
                next: DaemonOperationStatus::Failed,
                updated_at_unix_seconds: now_unix_seconds,
                event_kind_json: Some(&kind_json),
                event_retention_limit: self.event_journal.capacity(),
            }) {
            Ok(Some(record)) => {
                self.project_restores.remove(operation_id);
                if let Err(error) = self.event_journal.append_record(record) {
                    tracing::error!(operation_id, error = %error, "restore rejection journal failed");
                }
            }
            Ok(None) => tracing::error!(operation_id, "restore rejection omitted its event"),
            Err(error) => tracing::error!(
                operation_id,
                error = %error,
                "restore rejection persistence failed"
            ),
        }
    }

    fn publish_finished_project_restore(&mut self, now_unix_seconds: i64) {
        if !self
            .active_project_restore
            .as_ref()
            .is_some_and(ActiveProjectRestore::is_finished)
        {
            return;
        }
        let active = self
            .active_project_restore
            .take()
            .expect("finished project restore was present");
        let (operation_id, task) = active.into_parts();
        match self.engine_runtime.block_on(task) {
            Ok(result) => {
                if let Err(error) = publish_project_restore_result(
                    &mut self.control_plane,
                    &mut self.event_journal,
                    result,
                    now_unix_seconds,
                ) {
                    tracing::error!(operation_id, error, "project restore persistence failed");
                }
            }
            Err(error) => {
                let event = IpcEventKind::Failed {
                    code: "project_restore_task_failed".to_owned(),
                    message: error.to_string(),
                };
                let kind_json = serde_json::to_string(&event)
                    .expect("project restore task failure serialization is infallible");
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
                    tracing::error!(
                        operation_id,
                        error = %persistence_error,
                        "project restore task failure persistence failed"
                    );
                }
            }
        }
    }
}
