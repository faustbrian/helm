use super::ipc::IpcEventKind;
use super::{
    ActiveProjectCommand, EngineConnectionOutcome, ProjectCommandExecutionOptions,
    UnixDaemonRuntime, execute_queued_project_command, publish_project_command_result,
};
use crate::control_plane::ServiceDeploymentStrategy;
use crate::control_plane::engine::EngineError;
use crate::control_plane::state::{DaemonOperationStatus, DaemonOperationTransitionOptions};
use crate::control_plane::workload::{
    EphemeralBrowserOptions, EphemeralBrowserPlan, plan_ephemeral_browser,
};
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
        if self.active_project_command.is_some()
            || self.active_postgres_prune.is_some()
            || self.active_project_backup.is_some()
            || self.active_project_restore.is_some()
            || self.active_migration_decision.is_some()
        {
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
        let ephemeral_browser = self.ephemeral_browser_plan(&operation);
        let installation_id = self
            .global_network_request
            .metadata()
            .installation_id()
            .to_owned();
        let schema_version = self.global_network_request.metadata().schema_version();
        let task = self.engine_runtime.spawn(execute_queued_project_command(
            engine,
            ProjectCommandExecutionOptions {
                managed_environment: self.project_command_environment(&operation),
                operation,
                installation_id,
                schema_version,
                ephemeral_browser,
            },
        ));
        self.active_project_command = Some(ActiveProjectCommand::new(operation_id, task));
        self.engine_runtime.block_on(tokio::task::yield_now());
    }

    fn project_command_environment(
        &self,
        operation: &super::QueuedProjectCommand,
    ) -> Result<std::collections::BTreeMap<String, String>, EngineError> {
        self.control_plane
            .managed_environments()
            .map_err(invalid)?
            .into_iter()
            .find(|environment| {
                environment.project_id() == operation.plan().project_id()
                    && environment.lifecycle()
                        == crate::control_plane::state::EnvironmentLifecycle::Active
            })
            .map(|environment| environment.values().clone())
            .ok_or_else(|| {
                invalid(format!(
                    "project '{}' has no active managed environment",
                    operation.plan().project_id()
                ))
            })
    }

    fn ephemeral_browser_plan(
        &self,
        operation: &super::QueuedProjectCommand,
    ) -> Option<Result<EphemeralBrowserPlan, EngineError>> {
        if !operation.plan().browser_session() {
            return None;
        }

        Some((|| {
            let execution = self
                .engine_reconciliation
                .execution_plan()
                .ok_or_else(|| invalid("no complete desired registry is available"))?;
            let matches = execution
                .services()
                .iter()
                .filter(|service| {
                    service.project().as_str() == operation.plan().project_id()
                        && service.strategy() == ServiceDeploymentStrategy::Ephemeral
                })
                .collect::<Vec<_>>();
            let browser = match matches.as_slice() {
                [browser] => *browser,
                [] => {
                    return Err(invalid(format!(
                        "project '{}' has no declared Dusk or Selenium service",
                        operation.plan().project_id()
                    )));
                }
                _ => {
                    return Err(invalid(format!(
                        "project '{}' declares multiple browser services; select exactly one in configuration",
                        operation.plan().project_id()
                    )));
                }
            };
            let platform = super::unix_daemon_runtime::runtime_linux_platform().map_err(invalid)?;

            plan_ephemeral_browser(EphemeralBrowserOptions {
                service: browser,
                operation_id: operation.operation_id(),
                installation_id: self.global_network_request.metadata().installation_id(),
                schema_version: self.global_network_request.metadata().schema_version(),
                platform,
                network_name: self.global_network_request.name(),
            })
            .map_err(invalid)
        })())
    }

    pub(super) fn publish_finished_project_command(&mut self, now_unix_seconds: i64) {
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

fn invalid(detail: impl std::fmt::Display) -> EngineError {
    EngineError::InvalidRequest {
        detail: detail.to_string(),
    }
}
