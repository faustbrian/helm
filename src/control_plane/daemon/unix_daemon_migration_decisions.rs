use super::ipc::IpcEventKind;
use super::{
    ActiveMigrationDecision, EngineConnectionOutcome, MigrationDecisionExecutionOptions,
    UnixDaemonRuntime, execute_queued_migration_decision, publish_migration_decision_result,
};
use crate::control_plane::shared_infrastructure::{
    OsCredentialEntropy, SharedInstancePlan, resolve_execution_shared_instances,
};
use crate::control_plane::state::{DaemonOperationStatus, DaemonOperationTransitionOptions};
use std::time::{Duration, Instant};

const MIGRATION_DECISION_TIMEOUT: Duration = Duration::from_secs(10 * 60);

impl UnixDaemonRuntime {
    /// Reports whether an explicit migration decision is in flight.
    pub(super) const fn has_active_migration_decision(&self) -> bool {
        self.active_migration_decision.is_some()
    }

    /// Advances one exact confirm or rollback without overlapping mutation.
    pub(super) fn drive_migration_decisions(&mut self, now: Instant, now_unix_seconds: i64) {
        self.engine_runtime.block_on(tokio::task::yield_now());
        self.publish_finished_migration_decision(now_unix_seconds);
        if self.active_migration_decision.is_some()
            || self.active_postgres_prune.is_some()
            || self.active_project_restore.is_some()
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
                if self.migration_decisions.len() > 0 {
                    tracing::debug!(
                        attempt = retry.attempt(),
                        retry_milliseconds = retry.duration().as_millis(),
                        error = %detail,
                        "migration decision is waiting for the selected Engine"
                    );
                }

                return;
            }
            EngineConnectionOutcome::Connected | EngineConnectionOutcome::BackingOff { .. } => {}
        }

        let Some(engine) = self.engine_connection.engine().cloned() else {
            return;
        };
        let Some(operation) = self.migration_decisions.front() else {
            return;
        };
        let Some(shared) = self.migration_decision_shared_plan(operation) else {
            return;
        };
        let operation_id = operation.operation_id().to_owned();
        let shared = match shared {
            Ok(shared) => shared,
            Err(message) => {
                self.reject_queued_migration_decision(&operation_id, &message, now_unix_seconds);

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
                "migration decision could not claim its durable queue entry"
            );

            return;
        }
        let operation = self
            .migration_decisions
            .pop_front()
            .expect("durably claimed migration decision remains queued");
        let task = self.engine_runtime.spawn(execute_queued_migration_decision(
            engine,
            OsCredentialEntropy,
            MigrationDecisionExecutionOptions {
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
                timeout: MIGRATION_DECISION_TIMEOUT,
            },
        ));
        self.active_migration_decision = Some(ActiveMigrationDecision::new(operation_id, task));
        self.engine_runtime.block_on(tokio::task::yield_now());
    }

    fn migration_decision_shared_plan(
        &self,
        operation: &super::QueuedMigrationDecision,
    ) -> Option<Result<SharedInstancePlan, String>> {
        let execution = self.engine_reconciliation.execution_plan()?;
        let checkpoint = match self.control_plane.migrations() {
            Ok(migrations) => {
                let matches = migrations
                    .into_iter()
                    .filter(|migration| {
                        migration.migration_id() == operation.migration_id()
                            && migration.project_id() == operation.project_id()
                    })
                    .collect::<Vec<_>>();
                match matches.as_slice() {
                    [checkpoint] => checkpoint.clone(),
                    [] => {
                        return Some(Err(format!(
                            "migration '{}' has no exact durable checkpoint",
                            operation.migration_id()
                        )));
                    }
                    _ => {
                        return Some(Err(format!(
                            "migration '{}' matched multiple durable checkpoints",
                            operation.migration_id()
                        )));
                    }
                }
            }
            Err(error) => return Some(Err(error.to_string())),
        };
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
                    && plan.fingerprint().as_str() == checkpoint.source_compatibility_fingerprint()
            })
            .collect::<Vec<_>>();
        Some(match matches.as_slice() {
            [shared] => Ok(shared.clone()),
            [] => Err(format!(
                "migration '{}' has no exact source compatibility plan",
                operation.migration_id()
            )),
            _ => Err(format!(
                "migration '{}' matched multiple source compatibility plans",
                operation.migration_id()
            )),
        })
    }

    fn reject_queued_migration_decision(
        &mut self,
        operation_id: &str,
        message: &str,
        now_unix_seconds: i64,
    ) {
        let event = IpcEventKind::Failed {
            code: "migration_decision_plan_invalid".to_owned(),
            message: message.to_owned(),
        };
        let kind_json = serde_json::to_string(&event)
            .expect("migration decision plan failure serialization is infallible");
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
                self.migration_decisions.remove(operation_id);
                if let Err(error) = self.event_journal.append_record(record) {
                    tracing::error!(operation_id, error = %error, "decision rejection journal failed");
                }
            }
            Ok(None) => tracing::error!(operation_id, "decision rejection omitted its event"),
            Err(error) => tracing::error!(
                operation_id,
                error = %error,
                "decision rejection persistence failed"
            ),
        }
    }

    fn publish_finished_migration_decision(&mut self, now_unix_seconds: i64) {
        if !self
            .active_migration_decision
            .as_ref()
            .is_some_and(ActiveMigrationDecision::is_finished)
        {
            return;
        }
        let active = self
            .active_migration_decision
            .take()
            .expect("finished migration decision was present");
        let (operation_id, task) = active.into_parts();
        match self.engine_runtime.block_on(task) {
            Ok(result) => {
                if let Err(error) = publish_migration_decision_result(
                    &mut self.control_plane,
                    &mut self.event_journal,
                    result,
                    now_unix_seconds,
                ) {
                    tracing::error!(operation_id, error, "migration decision persistence failed");
                }
            }
            Err(error) => {
                let event = IpcEventKind::Failed {
                    code: "migration_decision_task_failed".to_owned(),
                    message: error.to_string(),
                };
                let kind_json = serde_json::to_string(&event)
                    .expect("migration decision task failure serialization is infallible");
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
                        "migration decision task failure persistence failed"
                    );
                }
            }
        }
    }
}
