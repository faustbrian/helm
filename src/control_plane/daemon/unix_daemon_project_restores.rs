use super::{
    ActiveProjectRestore, EngineConnectionOutcome, ProjectRestoreExecutionOptions,
    ProjectRestoreTargetPlan, UnixDaemonRuntime, execute_queued_project_restore,
    publish_project_restore_result,
};
use crate::control_plane::project_infrastructure::materialize_project_service_configurations;
use crate::control_plane::shared_infrastructure::{
    OsCredentialEntropy, resolve_execution_shared_instances,
};
use crate::control_plane::state::{DaemonOperationStatus, DaemonOperationTransitionOptions};
use crate::control_plane::workload::{
    DedicatedProjectServiceOptions, plan_dedicated_project_service,
};
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
            || self.active_postgres_prune.is_some()
            || self.active_project_command.is_some()
            || self.active_project_backup.is_some()
            || self.active_migration_decision.is_some()
            || self.has_active_scheduled_commands()
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
        let Some(operation) = self.project_restores.front().cloned() else {
            return;
        };
        let Some(target) = self.project_restore_target_plan(&operation) else {
            return;
        };
        let operation_id = operation.operation_id().to_owned();
        let target = match target {
            Ok(target) => target,
            Err(message) => {
                self.reject_queued_project_restore(&operation_id, &message, now_unix_seconds);

                return;
            }
        };
        let Some(operation) = self.project_restores.pop_front() else {
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
                "project restore could not claim its durable queue entry"
            );
            self.project_restores.requeue_front(operation);

            return;
        }
        let task = self.engine_runtime.spawn(execute_queued_project_restore(
            engine,
            OsCredentialEntropy,
            ProjectRestoreExecutionOptions {
                operation,
                target,
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

    fn project_restore_target_plan(
        &mut self,
        operation: &super::QueuedProjectRestore,
    ) -> Option<Result<ProjectRestoreTargetPlan, String>> {
        let execution = self.engine_reconciliation.execution_plan()?;
        let platform = match super::unix_daemon_runtime::runtime_linux_platform() {
            Ok(platform) => platform,
            Err(error) => return Some(Err(error)),
        };
        if operation.kind() == "volume" {
            let mut prepared = match self
                .control_plane
                .prepare_project_services(execution, &OsCredentialEntropy)
            {
                Ok(prepared) => prepared,
                Err(error) => return Some(Err(error.to_string())),
            };
            if let Err(error) =
                materialize_project_service_configurations(&mut prepared, &self.runtime_directory)
            {
                return Some(Err(error.to_string()));
            }
            let matches = execution
                .services()
                .iter()
                .filter(|service| {
                    service.project().as_str() == operation.project_id()
                        && service.service().as_str() == operation.service_id()
                })
                .filter_map(|service| {
                    let prepared = prepared.iter().find(|prepared| {
                        prepared.project_id() == service.project().as_str()
                            && prepared.service_id() == service.service().as_str()
                    });
                    plan_dedicated_project_service(DedicatedProjectServiceOptions {
                        service,
                        generated_environment: prepared
                            .map(|prepared| prepared.container_environment()),
                        generated_command: prepared
                            .and_then(|prepared| prepared.container_command()),
                        generated_configuration_mount: prepared
                            .and_then(|prepared| prepared.container_configuration_mount()),
                        provisioning_job: prepared.and_then(|prepared| prepared.provisioning_job()),
                        installation_id: self.global_network_request.metadata().installation_id(),
                        schema_version: self.global_network_request.metadata().schema_version(),
                        platform,
                        network_name: self.global_network_request.name(),
                    })
                    .ok()
                })
                .filter(|plan| {
                    plan.request().metadata().compatibility_fingerprint()
                        == operation.compatibility_fingerprint()
                        && plan.volume().is_some_and(|volume| {
                            volume.name() == operation.logical_resource_id()
                                && volume.metadata().compatibility_fingerprint()
                                    == operation.compatibility_fingerprint()
                        })
                })
                .collect::<Vec<_>>();
            return Some(match matches.as_slice() {
                [plan] => Ok(ProjectRestoreTargetPlan::Dedicated(plan.clone())),
                [] => Err(format!(
                    "recovery point '{}' has no exact dedicated service plan",
                    operation.recovery_point_id()
                )),
                _ => Err(format!(
                    "recovery point '{}' matched multiple dedicated service plans",
                    operation.recovery_point_id()
                )),
            });
        }
        let shared = match resolve_execution_shared_instances(execution, platform) {
            Ok(shared) => shared,
            Err(error) => return Some(Err(error.to_string())),
        };
        let implementation = match operation.kind() {
            "postgres_database_and_role" => "postgresql",
            "mysql_database" => "mysql",
            "mariadb_database" => "mariadb",
            "mongodb_database" => "mongodb",
            "sqlserver_database" => "sqlserver",
            "redis_acl_prefix" => "redis",
            "valkey_acl_prefix" => "valkey",
            "minio_bucket_policy" => "minio",
            "rabbitmq_vhost_user" => "rabbitmq",
            kind => {
                return Some(Err(format!(
                    "recovery point '{}' has unsupported resource kind '{kind}'",
                    operation.recovery_point_id()
                )));
            }
        };
        let matches = shared
            .into_iter()
            .filter(|plan| {
                plan.profile().implementation() == implementation
                    && plan.fingerprint().as_str() == operation.compatibility_fingerprint()
            })
            .collect::<Vec<_>>();
        Some(match matches.as_slice() {
            [shared] => Ok(ProjectRestoreTargetPlan::Shared(shared.clone())),
            [] => Err(format!(
                "recovery point '{}' has no exact {implementation} compatibility plan",
                operation.recovery_point_id(),
            )),
            _ => Err(format!(
                "recovery point '{}' matched multiple {implementation} compatibility plans",
                operation.recovery_point_id(),
            )),
        })
    }

    fn reject_queued_project_restore(
        &mut self,
        operation_id: &str,
        message: &str,
        now_unix_seconds: i64,
    ) {
        let kind_json =
            super::failed_event_json("project_restore_plan_invalid", message, operation_id);
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

    pub(super) fn publish_finished_project_restore(&mut self, now_unix_seconds: i64) {
        if !self
            .active_project_restore
            .as_ref()
            .is_some_and(ActiveProjectRestore::is_finished)
        {
            return;
        }
        let Some(active) = self.active_project_restore.take() else {
            return;
        };
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
                let kind_json = super::failed_event_json(
                    "project_restore_task_failed",
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
