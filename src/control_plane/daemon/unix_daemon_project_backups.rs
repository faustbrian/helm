use super::{
    ActiveProjectBackup, EngineConnectionOutcome, ProjectBackupExecutionOptions, UnixDaemonRuntime,
    execute_queued_project_backup, publish_project_backup_result,
};
use crate::control_plane::engine::EngineError;
use crate::control_plane::state::{
    CredentialLifecycle, DaemonOperationStatus, DaemonOperationTransitionOptions, ResourceLifecycle,
};
use std::time::{Duration, Instant};

const PROJECT_BACKUP_TIMEOUT: Duration = Duration::from_secs(10 * 60);

impl UnixDaemonRuntime {
    /// Reports whether a recovery-point Engine operation is in flight.
    pub(super) const fn has_active_project_backup(&self) -> bool {
        self.active_project_backup.is_some()
    }

    /// Advances one bounded backup without overlapping other Engine mutations.
    pub(super) fn drive_project_backups(&mut self, now: Instant, now_unix_seconds: i64) {
        self.engine_runtime.block_on(tokio::task::yield_now());
        self.publish_finished_project_backup(now_unix_seconds);
        if self.active_project_backup.is_some()
            || self.active_postgres_prune.is_some()
            || self.active_project_command.is_some()
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
                if self.project_backups.len() > 0 {
                    tracing::debug!(
                        attempt = retry.attempt(),
                        retry_milliseconds = retry.duration().as_millis(),
                        error = %detail,
                        "project backup is waiting for the selected Engine"
                    );
                }

                return;
            }
            EngineConnectionOutcome::Connected | EngineConnectionOutcome::BackingOff { .. } => {}
        }

        let Some(engine) = self.engine_connection.engine().cloned() else {
            return;
        };
        let Some(operation) = self.project_backups.pop_front() else {
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
            tracing::error!(
                operation_id,
                error = %error,
                "project backup could not claim its durable queue entry"
            );
            self.project_backups.requeue_front(operation);

            return;
        }
        let logical_resource = self.project_backup_logical_resource(&operation);
        let credential = self.project_backup_credential(&operation);
        let administrator = self.project_backup_administrator(&operation);
        let physical_resource = self.project_backup_physical_resource(&operation);
        let installation_id = self
            .global_network_request
            .metadata()
            .installation_id()
            .to_owned();
        let schema_version = self.global_network_request.metadata().schema_version();
        let task = self.engine_runtime.spawn(execute_queued_project_backup(
            engine,
            ProjectBackupExecutionOptions {
                operation,
                logical_resource,
                credential,
                administrator,
                physical_resource,
                installation_id,
                schema_version,
                backup_root: self.runtime_directory.join("backups"),
                created_at_unix_seconds: now_unix_seconds,
                timeout: PROJECT_BACKUP_TIMEOUT,
            },
        ));
        self.active_project_backup = Some(ActiveProjectBackup::new(operation_id, task));
        self.engine_runtime.block_on(tokio::task::yield_now());
    }

    fn project_backup_logical_resource(
        &self,
        operation: &super::QueuedProjectBackup,
    ) -> Result<crate::control_plane::state::LogicalResourceRecord, EngineError> {
        let matches = self
            .control_plane
            .logical_resources()
            .map_err(invalid)?
            .into_iter()
            .filter(|logical| {
                logical.logical_resource_id() == operation.logical_resource_id()
                    && logical.project_id() == operation.project_id()
                    && logical.service_id() == operation.service_id()
                    && logical.kind() == operation.kind()
                    && logical.compatibility_fingerprint() == operation.compatibility_fingerprint()
                    && logical.lifecycle() == ResourceLifecycle::Active
            })
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [logical] => Ok(logical.clone()),
            [] => Err(invalid(
                "project backup has no exact active logical resource",
            )),
            _ => Err(invalid(
                "project backup matched multiple active logical resources",
            )),
        }
    }

    fn project_backup_credential(
        &self,
        operation: &super::QueuedProjectBackup,
    ) -> Result<crate::control_plane::state::CredentialRecord, EngineError> {
        let matches = self
            .control_plane
            .credentials()
            .map_err(invalid)?
            .into_iter()
            .filter(|credential| {
                credential.project_id() == Some(operation.project_id())
                    && credential.service_id() == operation.service_id()
                    && credential.lifecycle() == CredentialLifecycle::Active
            })
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [credential] => Ok(credential.clone()),
            [] => Err(invalid("project backup has no exact active credential")),
            _ => Err(invalid(
                "project backup matched multiple active credentials",
            )),
        }
    }

    fn project_backup_administrator(
        &self,
        operation: &super::QueuedProjectBackup,
    ) -> Result<Option<crate::control_plane::state::CredentialRecord>, EngineError> {
        let implementation = match operation.kind() {
            "redis_acl_prefix" => "redis",
            "valkey_acl_prefix" => "valkey",
            _ => return Ok(None),
        };
        let fingerprint = operation
            .compatibility_fingerprint()
            .strip_prefix("sha256:")
            .ok_or_else(|| invalid("project backup compatibility fingerprint is malformed"))?;
        let credential_id = format!("shared/{fingerprint}/{implementation}-bootstrap");
        let matches = self
            .control_plane
            .credentials()
            .map_err(invalid)?
            .into_iter()
            .filter(|credential| {
                credential.credential_id() == credential_id
                    && credential.project_id().is_none()
                    && credential.service_id() == implementation
                    && credential.username() == "stackctl_admin"
                    && credential.lifecycle() == CredentialLifecycle::Active
            })
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [administrator] => Ok(Some(administrator.clone())),
            [] => Err(invalid(
                "project backup has no exact active shared administrator",
            )),
            _ => Err(invalid(
                "project backup matched multiple active shared administrators",
            )),
        }
    }

    fn project_backup_physical_resource(
        &self,
        operation: &super::QueuedProjectBackup,
    ) -> Result<Option<crate::control_plane::state::ResourceRecord>, EngineError> {
        if operation.kind() != "volume" {
            return Ok(None);
        }
        let matches = self
            .control_plane
            .resources()
            .map_err(invalid)?
            .into_iter()
            .filter(|resource| {
                resource.resource_id() == operation.logical_resource_id()
                    && resource.project_id() == Some(operation.project_id())
                    && resource.scope_id() == Some(operation.service_id())
                    && resource.kind() == operation.kind()
                    && resource.compatibility_fingerprint() == operation.compatibility_fingerprint()
                    && resource.retention()
                        == crate::control_plane::state::ResourceRetention::Persistent
                    && resource.lifecycle() == ResourceLifecycle::Active
            })
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [resource] => Ok(Some(resource.clone())),
            [] => Err(invalid(
                "project backup has no exact active persistent volume",
            )),
            _ => Err(invalid(
                "project backup matched multiple active persistent volumes",
            )),
        }
    }

    pub(super) fn publish_finished_project_backup(&mut self, now_unix_seconds: i64) {
        if !self
            .active_project_backup
            .as_ref()
            .is_some_and(ActiveProjectBackup::is_finished)
        {
            return;
        }
        let Some(active) = self.active_project_backup.take() else {
            return;
        };
        let (operation_id, task) = active.into_parts();
        match self.engine_runtime.block_on(task) {
            Ok(result) => {
                if let Err(error) = publish_project_backup_result(
                    &mut self.control_plane,
                    &mut self.event_journal,
                    result,
                    now_unix_seconds,
                ) {
                    tracing::error!(operation_id, error, "project backup persistence failed");
                }
            }
            Err(error) => {
                let kind_json = super::failed_event_json(
                    "project_backup_task_failed",
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
                        "project backup task failure persistence failed"
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
