use super::dispatch_daemon_request::prepare_project_backup;
use super::unix_daemon_automatic_workflows::{
    AutomaticOperationDisposition, automatic_operation_disposition,
};
use super::{
    IpcMigrationDecision, QueuedMigrationDecision, QueuedProjectRestore,
    QueuedProjectRestoreOptions, UnixDaemonRuntime,
};
use crate::control_plane::shared_infrastructure::resolve_execution_shared_instances;
use crate::control_plane::state::{
    InstallationLifecycle, LogicalResourceRecord, MigrationPhase, ResourceLifecycle,
};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const AUTOMATIC_POSTGRES_UPGRADE_PREFIX: &str = "automatic-postgres-upgrade:";

#[derive(Clone, Debug, Eq, PartialEq)]
struct AutomaticPostgresUpgrade {
    project_directory: PathBuf,
    project_id: String,
    service_id: String,
    logical_resource_id: String,
    source_fingerprint: String,
    target_fingerprint: String,
}

impl AutomaticPostgresUpgrade {
    fn operation_id(&self, action: &str) -> String {
        let mut digest = Sha256::new();
        for value in [
            self.project_id.as_str(),
            self.service_id.as_str(),
            self.source_fingerprint.as_str(),
            self.target_fingerprint.as_str(),
        ] {
            digest.update(value.as_bytes());
            digest.update([0]);
        }

        format!(
            "{AUTOMATIC_POSTGRES_UPGRADE_PREFIX}{}:{action}",
            hex::encode(digest.finalize())
        )
    }
}

impl UnixDaemonRuntime {
    /// Advances safe PostgreSQL major upgrades through backup, cutover, and
    /// retained-source retirement without operator intervention.
    pub(super) fn schedule_automatic_postgres_upgrades(&mut self, now_unix_seconds: i64) {
        if matches!(
            self.control_plane.installation_lifecycle(),
            Ok(Some(
                InstallationLifecycle::Deleting | InstallationLifecycle::Deleted
            ))
        ) {
            return;
        }
        let Some(execution) = self.engine_reconciliation.execution_plan().cloned() else {
            return;
        };
        let platform = match super::unix_daemon_runtime::runtime_linux_platform() {
            Ok(platform) => platform,
            Err(error) => {
                tracing::error!(
                    error,
                    "automatic PostgreSQL upgrade platform is unavailable"
                );
                return;
            }
        };
        let shared = match resolve_execution_shared_instances(&execution, platform) {
            Ok(shared) => shared,
            Err(error) => {
                tracing::error!(error = %error, "automatic PostgreSQL upgrade planning failed");
                return;
            }
        };
        let logical_resources = match self.control_plane.logical_resources() {
            Ok(logical_resources) => logical_resources,
            Err(error) => {
                tracing::error!(error = %error, "automatic PostgreSQL inventory failed");
                return;
            }
        };
        let upgrades = automatic_postgres_upgrades(&execution, &shared, &logical_resources);

        for upgrade in upgrades {
            if !self.ensure_automatic_postgres_backup(&upgrade, now_unix_seconds) {
                continue;
            }
            self.ensure_automatic_postgres_restore(&upgrade, now_unix_seconds);
        }

        self.schedule_automatic_postgres_confirmations(now_unix_seconds);
    }

    fn ensure_automatic_postgres_backup(
        &mut self,
        upgrade: &AutomaticPostgresUpgrade,
        now_unix_seconds: i64,
    ) -> bool {
        let operation_id = upgrade.operation_id("backup");
        let existing = match self.automatic_operation(&operation_id) {
            Ok(existing) => existing,
            Err(error) => {
                tracing::error!(
                    operation_id,
                    error,
                    "automatic PostgreSQL backup lookup failed"
                );
                return false;
            }
        };
        let disposition = automatic_operation_disposition(existing.as_ref(), now_unix_seconds);
        match disposition {
            AutomaticOperationDisposition::Completed => return true,
            AutomaticOperationDisposition::Wait => return false,
            AutomaticOperationDisposition::EnqueueNew
            | AutomaticOperationDisposition::RetryFailed => {}
        }
        let queued = match prepare_project_backup(
            &self.control_plane,
            &operation_id,
            &upgrade.project_directory,
            &upgrade.service_id,
        ) {
            Ok(queued) => queued,
            Err(error) => {
                tracing::error!(
                    operation_id,
                    error,
                    "automatic PostgreSQL backup planning failed"
                );
                return false;
            }
        };
        let payload = match queued.payload_json() {
            Ok(payload) => payload,
            Err(error) => {
                tracing::error!(
                    operation_id,
                    error,
                    "automatic PostgreSQL backup encoding failed"
                );
                return false;
            }
        };
        if let Err(error) = self.project_backups.enqueue(queued) {
            tracing::error!(operation_id, error = %error, "automatic PostgreSQL backup queue failed");
            return false;
        }
        let persistence = match disposition {
            AutomaticOperationDisposition::RetryFailed => self.retry_automatic_operation(
                &operation_id,
                "project_backup",
                &payload,
                now_unix_seconds,
            ),
            _ => self.persist_automatic_operation(
                &operation_id,
                "project_backup",
                payload,
                now_unix_seconds,
            ),
        };
        if let Err(error) = persistence {
            drop(self.project_backups.remove(&operation_id));
            tracing::error!(
                operation_id,
                error,
                "automatic PostgreSQL backup persistence failed"
            );
        }

        false
    }

    fn ensure_automatic_postgres_restore(
        &mut self,
        upgrade: &AutomaticPostgresUpgrade,
        now_unix_seconds: i64,
    ) {
        let operation_id = upgrade.operation_id("restore");
        let existing = match self.automatic_operation(&operation_id) {
            Ok(existing) => existing,
            Err(error) => {
                tracing::error!(
                    operation_id,
                    error,
                    "automatic PostgreSQL restore lookup failed"
                );
                return;
            }
        };
        let disposition = automatic_operation_disposition(existing.as_ref(), now_unix_seconds);
        if matches!(
            disposition,
            AutomaticOperationDisposition::Completed | AutomaticOperationDisposition::Wait
        ) {
            return;
        }
        let backup_id = upgrade.operation_id("backup");
        let queued = match QueuedProjectRestore::new(QueuedProjectRestoreOptions {
            operation_id: operation_id.clone(),
            recovery_point_id: backup_id,
            project_id: upgrade.project_id.clone(),
            service_id: upgrade.service_id.clone(),
            logical_resource_id: upgrade.logical_resource_id.clone(),
            kind: "postgres_database_and_role".to_owned(),
            source_compatibility_fingerprint: upgrade.source_fingerprint.clone(),
            target_compatibility_fingerprint: upgrade.target_fingerprint.clone(),
        }) {
            Ok(queued) => queued,
            Err(error) => {
                tracing::error!(
                    operation_id,
                    error,
                    "automatic PostgreSQL restore planning failed"
                );
                return;
            }
        };
        let payload = match queued.payload_json() {
            Ok(payload) => payload,
            Err(error) => {
                tracing::error!(
                    operation_id,
                    error,
                    "automatic PostgreSQL restore encoding failed"
                );
                return;
            }
        };
        if let Err(error) = self.project_restores.enqueue(queued) {
            tracing::error!(operation_id, error = %error, "automatic PostgreSQL restore queue failed");
            return;
        }
        let persistence = match disposition {
            AutomaticOperationDisposition::RetryFailed => self.retry_automatic_operation(
                &operation_id,
                "project_restore",
                &payload,
                now_unix_seconds,
            ),
            _ => self.persist_automatic_operation(
                &operation_id,
                "project_restore",
                payload,
                now_unix_seconds,
            ),
        };
        if let Err(error) = persistence {
            drop(self.project_restores.remove(&operation_id));
            tracing::error!(
                operation_id,
                error,
                "automatic PostgreSQL restore persistence failed"
            );
        }
    }

    fn schedule_automatic_postgres_confirmations(&mut self, now_unix_seconds: i64) {
        let migrations = match self.control_plane.migrations() {
            Ok(migrations) => migrations,
            Err(error) => {
                tracing::error!(error = %error, "automatic PostgreSQL confirmation inventory failed");
                return;
            }
        };
        for migration in migrations.into_iter().filter(|migration| {
            migration.phase() == MigrationPhase::Cutover
                && migration
                    .migration_id()
                    .starts_with(AUTOMATIC_POSTGRES_UPGRADE_PREFIX)
                && migration.migration_id().ends_with(":restore")
        }) {
            let operation_id = migration
                .migration_id()
                .strip_suffix(":restore")
                .map(|prefix| format!("{prefix}:confirm"))
                .unwrap_or_default();
            let existing = match self.automatic_operation(&operation_id) {
                Ok(existing) => existing,
                Err(error) => {
                    tracing::error!(
                        operation_id,
                        error,
                        "automatic PostgreSQL confirmation lookup failed"
                    );
                    continue;
                }
            };
            let disposition = automatic_operation_disposition(existing.as_ref(), now_unix_seconds);
            if matches!(
                disposition,
                AutomaticOperationDisposition::Completed | AutomaticOperationDisposition::Wait
            ) {
                continue;
            }
            let queued = match QueuedMigrationDecision::new(
                operation_id.clone(),
                migration.migration_id().to_owned(),
                migration.project_id().to_owned(),
                IpcMigrationDecision::Confirm,
            ) {
                Ok(queued) => queued,
                Err(error) => {
                    tracing::error!(
                        operation_id,
                        error,
                        "automatic PostgreSQL confirmation planning failed"
                    );
                    continue;
                }
            };
            let payload = match queued.payload_json() {
                Ok(payload) => payload,
                Err(error) => {
                    tracing::error!(
                        operation_id,
                        error,
                        "automatic PostgreSQL confirmation encoding failed"
                    );
                    continue;
                }
            };
            if let Err(error) = self.migration_decisions.enqueue(queued) {
                tracing::error!(operation_id, error = %error, "automatic PostgreSQL confirmation queue failed");
                continue;
            }
            let persistence = match disposition {
                AutomaticOperationDisposition::RetryFailed => self.retry_automatic_operation(
                    &operation_id,
                    "migration_decision",
                    &payload,
                    now_unix_seconds,
                ),
                _ => self.persist_automatic_operation(
                    &operation_id,
                    "migration_decision",
                    payload,
                    now_unix_seconds,
                ),
            };
            if let Err(error) = persistence {
                drop(self.migration_decisions.remove(&operation_id));
                tracing::error!(
                    operation_id,
                    error,
                    "automatic PostgreSQL confirmation persistence failed"
                );
            }
        }
    }
}

fn automatic_postgres_upgrades(
    execution: &crate::control_plane::ExecutionPlan,
    shared: &[crate::control_plane::shared_infrastructure::SharedInstancePlan],
    logical_resources: &[LogicalResourceRecord],
) -> Vec<AutomaticPostgresUpgrade> {
    let mut upgrades = Vec::new();
    for target in shared
        .iter()
        .filter(|target| target.profile().implementation() == "postgresql")
    {
        for consumer in target.consumers() {
            let sources = logical_resources
                .iter()
                .filter(|logical| {
                    logical.project_id() == consumer.project_id()
                        && logical.service_id() == consumer.service_id()
                        && logical.kind() == "postgres_database_and_role"
                        && logical.lifecycle() == ResourceLifecycle::Active
                        && logical.compatibility_fingerprint() != target.fingerprint().as_str()
                })
                .collect::<Vec<_>>();
            let [source] = sources.as_slice() else {
                continue;
            };
            let Some(service) = execution.services().iter().find(|service| {
                service.project().as_str() == consumer.project_id()
                    && service.service().as_str() == consumer.service_id()
            }) else {
                continue;
            };
            upgrades.push(AutomaticPostgresUpgrade {
                project_directory: service.project_directory().to_path_buf(),
                project_id: source.project_id().to_owned(),
                service_id: source.service_id().to_owned(),
                logical_resource_id: source.logical_resource_id().to_owned(),
                source_fingerprint: source.compatibility_fingerprint().to_owned(),
                target_fingerprint: target.fingerprint().as_str().to_owned(),
            });
        }
    }
    upgrades
}

#[cfg(test)]
mod tests {
    use super::{AutomaticPostgresUpgrade, automatic_postgres_upgrades};
    use crate::control_plane::application::{ProjectSource, plan_project_registry};
    use crate::control_plane::resolve_execution_plan;
    use crate::control_plane::shared_infrastructure::resolve_execution_shared_instances;
    use crate::control_plane::state::{
        LogicalResourceRecord, LogicalResourceRecordOptions, ResourceLifecycle,
    };
    use std::path::PathBuf;

    #[test]
    fn automatic_upgrade_operation_ids_are_stable_and_stage_specific() {
        let upgrade = AutomaticPostgresUpgrade {
            project_directory: PathBuf::from("/workspace/api"),
            project_id: "api".to_owned(),
            service_id: "database".to_owned(),
            logical_resource_id: "api/database".to_owned(),
            source_fingerprint: "sha256:postgres-17".to_owned(),
            target_fingerprint: "sha256:postgres-18".to_owned(),
        };

        let backup = upgrade.operation_id("backup");
        let restore = upgrade.operation_id("restore");

        assert!(backup.starts_with("automatic-postgres-upgrade:"));
        assert!(backup.ends_with(":backup"));
        assert!(restore.ends_with(":restore"));
        assert_ne!(backup, restore);
        assert_eq!(backup, upgrade.operation_id("backup"));
    }

    #[test]
    fn desired_postgres_change_plans_one_cross_version_upgrade() {
        let image = concat!(
            "postgres@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        let source = ProjectSource::new(
            PathBuf::from("/work/api"),
            PathBuf::from("/work/api/.stackctl.yaml"),
            format!(
                "schema_version: 8\nproject: api\nservices:\n  database:\n    preset: postgres\n    version: \"18\"\n    image: {image}\n"
            ),
        );
        let registry = plan_project_registry(&[source]).expect("desired registry");
        let execution = resolve_execution_plan(&registry).expect("execution plan");
        let shared = resolve_execution_shared_instances(&execution, "linux/arm64")
            .expect("shared PostgreSQL plan");
        let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
            logical_resource_id: "api/database".to_owned(),
            shared_resource_id: "postgres-17".to_owned(),
            project_id: "api".to_owned(),
            service_id: "database".to_owned(),
            kind: "postgres_database_and_role".to_owned(),
            compatibility_fingerprint: "sha256:postgres-17".to_owned(),
            desired_revision: "sha256:old".to_owned(),
            lifecycle: ResourceLifecycle::Active,
            orphaned_at_unix_seconds: None,
        });

        let upgrades = automatic_postgres_upgrades(&execution, &shared, &[logical]);

        assert_eq!(upgrades.len(), 1);
        assert_eq!(upgrades[0].project_directory, PathBuf::from("/work/api"));
        assert_eq!(upgrades[0].source_fingerprint, "sha256:postgres-17");
        assert_eq!(
            upgrades[0].target_fingerprint,
            shared[0].fingerprint().as_str()
        );
        assert_ne!(
            upgrades[0].source_fingerprint,
            upgrades[0].target_fingerprint
        );
    }
}
