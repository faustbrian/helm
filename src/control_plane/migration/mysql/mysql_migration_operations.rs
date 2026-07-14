use super::{
    MySqlBackupOptions, MySqlMigrationOperationsOptions, MySqlRestoreOptions,
    MySqlVerifyTargetOptions, backup_mysql_database, restore_mysql_database, verify_mysql_target,
};
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, run_attached_command,
};
use crate::control_plane::migration::{
    MigrationBackup, MigrationCutoverPlan, MigrationFuture, MigrationOperationError,
    MigrationOperations, MigrationRollbackPlan, MigrationTargetPlan,
};
use crate::control_plane::shared_infrastructure::{MySqlFlavor, provision_mysql_logical_resource};
use crate::control_plane::state::{
    CredentialLifecycle, EnvironmentLifecycle, MigrationPhase, MigrationRecord, ResourceLifecycle,
};
use std::collections::BTreeMap;

/// MySQL/MariaDB implementation of every reversible migration operation.
pub(crate) struct MySqlMigrationOperations<'operation, E> {
    executor: &'operation E,
    options: MySqlMigrationOperationsOptions<'operation>,
}

impl<'operation, E> MySqlMigrationOperations<'operation, E>
where
    E: CommandExecutor + Sync,
{
    pub(crate) fn new(
        executor: &'operation E,
        options: MySqlMigrationOperationsOptions<'operation>,
    ) -> Result<Self, MigrationOperationError> {
        validate(&options)?;

        Ok(Self { executor, options })
    }
}

impl<E> MigrationOperations for MySqlMigrationOperations<'_, E>
where
    E: CommandExecutor + Sync,
{
    fn backup<'operation>(
        &'operation mut self,
        checkpoint: &'operation MigrationRecord,
    ) -> MigrationFuture<'operation, MigrationBackup> {
        let validation = validate_source_checkpoint(
            checkpoint,
            self.options.source_logical_resource,
            MigrationPhase::Inventoried,
        );
        let options = MySqlBackupOptions {
            flavor: self.options.flavor,
            logical_resource: self.options.source_logical_resource,
            credential: self.options.source_credential,
            database_name: self.options.source_logical_resource.logical_resource_id(),
            installation_id: self.options.installation_id,
            created_at_unix_seconds: self.options.operation_unix_seconds,
            backup_root: self.options.backup_root,
            timeout: self.options.timeout,
        };
        let executor = self.executor;
        let container = self.options.source_container;
        Box::pin(async move {
            validation?;
            backup_mysql_database(executor, container, &options).await
        })
    }

    fn provision_target<'operation>(
        &'operation mut self,
        checkpoint: &'operation MigrationRecord,
    ) -> MigrationFuture<'operation, MigrationTargetPlan> {
        let valid = validate_source_checkpoint(
            checkpoint,
            self.options.source_logical_resource,
            MigrationPhase::BackupVerified,
        );
        let executor = self.executor;
        let container = self.options.target_container;
        let instance = self.options.target_instance;
        let plan = self.options.target_plan;
        let logical = self.options.target_logical_resource.clone();
        let credential = self.options.target_credential.clone();
        Box::pin(async move {
            valid?;
            provision_mysql_logical_resource(executor, container, instance, plan)
                .await
                .map_err(|error| {
                    operation_error("MySQL-family target provisioning failed", error)
                })?;
            MigrationTargetPlan::new(plan.schema_name(), logical, credential)
        })
    }

    fn restore<'operation>(
        &'operation mut self,
        checkpoint: &'operation MigrationRecord,
        _backup_reference: &'operation str,
        target_resource_id: &'operation str,
    ) -> MigrationFuture<'operation, ()> {
        let options = MySqlRestoreOptions {
            flavor: self.options.flavor,
            checkpoint,
            source_logical_resource: self.options.source_logical_resource,
            credential: self.options.target_credential,
            installation_id: self.options.installation_id,
            target_database_name: target_resource_id,
            verified_at_unix_seconds: self.options.operation_unix_seconds,
            timeout: self.options.timeout,
        };
        let executor = self.executor;
        let container = self.options.target_container;
        Box::pin(async move { restore_mysql_database(executor, container, &options).await })
    }

    fn verify_target<'operation>(
        &'operation mut self,
        checkpoint: &'operation MigrationRecord,
        target_resource_id: &'operation str,
    ) -> MigrationFuture<'operation, ()> {
        let options = MySqlVerifyTargetOptions {
            flavor: self.options.flavor,
            checkpoint,
            credential: self.options.target_credential,
            installation_id: self.options.installation_id,
            target_database_name: target_resource_id,
            timeout: self.options.timeout,
        };
        let executor = self.executor;
        let container = self.options.target_container;
        Box::pin(async move { verify_mysql_target(executor, container, &options).await })
    }

    fn plan_cutover<'operation>(
        &'operation mut self,
        checkpoint: &'operation MigrationRecord,
        target_resource_id: &'operation str,
        rollback_reference: &'operation str,
    ) -> MigrationFuture<'operation, MigrationCutoverPlan> {
        let cutover = self.options.cutover.clone();
        let valid = checkpoint.phase() == MigrationPhase::TargetVerified
            && checkpoint.project_id() == self.options.target_logical_resource.project_id()
            && checkpoint.target_compatibility_fingerprint()
                == self
                    .options
                    .target_logical_resource
                    .compatibility_fingerprint()
            && checkpoint.target_resource_id() == Some(target_resource_id)
            && target_resource_id == self.options.target_plan.schema_name()
            && checkpoint.rollback_reference() == Some(rollback_reference);
        Box::pin(async move {
            if !valid {
                return Err(MigrationOperationError::new(
                    "MySQL-family cutover plan does not match its verified checkpoint",
                ));
            }
            Ok(cutover)
        })
    }

    fn plan_rollback<'operation>(
        &'operation mut self,
        inventory: &'operation MigrationRecord,
        checkpoint: &'operation MigrationRecord,
    ) -> MigrationFuture<'operation, MigrationRollbackPlan> {
        let rollback = self.options.rollback.clone();
        let valid = inventory.phase() == MigrationPhase::Inventoried
            && checkpoint.has_same_identity(inventory)
            && checkpoint.rollback_reference() == inventory.rollback_reference()
            && !matches!(
                checkpoint.phase(),
                MigrationPhase::Confirmed | MigrationPhase::RolledBack
            );
        Box::pin(async move {
            if !valid {
                return Err(MigrationOperationError::new(
                    "MySQL-family rollback plan does not match a reversible checkpoint",
                ));
            }
            Ok(rollback)
        })
    }

    fn retire_source<'operation>(
        &'operation mut self,
        inventory: &'operation MigrationRecord,
        checkpoint: &'operation MigrationRecord,
    ) -> MigrationFuture<'operation, ()> {
        let validation = validate_retirement(inventory, checkpoint, &self.options);
        let flavor = self.options.flavor;
        let database_name = self
            .options
            .source_environment
            .values()
            .get("DB_DATABASE")
            .cloned();
        let username = self.options.source_credential.username().to_owned();
        let request = CommandRequest::new(
            vec![
                client_executable(flavor).to_owned(),
                "--protocol=socket".to_owned(),
                "--user=root".to_owned(),
                "--batch".to_owned(),
                "--skip-column-names".to_owned(),
            ],
            BTreeMap::from([(
                "MYSQL_PWD".to_owned(),
                self.options.source_administrator.secret().to_owned(),
            )]),
            None,
        )
        .map_err(|error| operation_error("MySQL-family retirement request is invalid", error));
        let executor = self.executor;
        let container = self.options.source_container;
        let timeout = self.options.timeout;
        Box::pin(async move {
            validation?;
            let database_name = database_name.ok_or_else(|| {
                MigrationOperationError::new(
                    "MySQL-family source environment does not contain DB_DATABASE",
                )
            })?;
            let request = request?;
            let command = AttachedCommandOptions::new(
                request,
                retirement_sql(&database_name, &username).into_bytes(),
                "retire confirmed MySQL-family source",
                timeout,
            )
            .map_err(|error| {
                operation_error("MySQL-family retirement request is invalid", error)
            })?;
            run_attached_command(executor, container, &command)
                .await
                .map_err(|error| operation_error("MySQL-family source retirement failed", error))
        })
    }
}

fn validate(options: &MySqlMigrationOperationsOptions<'_>) -> Result<(), MigrationOperationError> {
    let source = options.source_logical_resource;
    let target = options.target_logical_resource;
    let expected_kind = match options.flavor {
        MySqlFlavor::MySql => "mysql_database",
        MySqlFlavor::MariaDb => "mariadb_database",
    };
    let retained = options.rollback.retained_targets().iter().any(|candidate| {
        candidate.logical_resource_id() == target.logical_resource_id()
            && candidate.shared_resource_id() == target.shared_resource_id()
            && candidate.lifecycle() == ResourceLifecycle::Retained
    });
    let invalid = options.installation_id.is_empty()
        || !options.backup_root.is_absolute()
        || options.operation_unix_seconds < 0
        || options.timeout.is_zero()
        || source.kind() != expected_kind
        || target.kind() != expected_kind
        || source.lifecycle() != ResourceLifecycle::Active
        || target.lifecycle() != ResourceLifecycle::Active
        || source.project_id() != target.project_id()
        || source.service_id() != target.service_id()
        || options.source_credential.project_id() != Some(source.project_id())
        || options.source_credential.service_id() != source.service_id()
        || options.source_credential.lifecycle() != CredentialLifecycle::Active
        || options.target_credential.project_id() != Some(target.project_id())
        || options.target_credential.service_id() != target.service_id()
        || options.target_credential.username() != options.target_plan.username()
        || options.target_credential.lifecycle() != CredentialLifecycle::Active
        || target.logical_resource_id()
            != format!("{}/{}", target.project_id(), target.service_id())
        || options.target_plan.schema_name() != source.logical_resource_id()
        || options.source_administrator.project_id().is_some()
        || options.source_administrator.username() != "root"
        || options.source_administrator.lifecycle() != CredentialLifecycle::Active
        || options.source_environment.lifecycle() != EnvironmentLifecycle::Active
        || options.cutover.project().project_name() != target.project_id()
        || options.rollback.project().project_name() != target.project_id()
        || !retained
        || options.source_container.metadata().installation_id() != options.installation_id
        || options.target_container.metadata().installation_id() != options.installation_id
        || options.source_container.id() == options.target_container.id()
        || options
            .source_container
            .metadata()
            .compatibility_fingerprint()
            != source.compatibility_fingerprint()
        || options
            .target_container
            .metadata()
            .compatibility_fingerprint()
            != target.compatibility_fingerprint();
    if invalid {
        return Err(MigrationOperationError::new(
            "MySQL-family migration operations do not describe one exact reversible resource",
        ));
    }

    Ok(())
}

fn validate_source_checkpoint(
    checkpoint: &MigrationRecord,
    source: &crate::control_plane::state::LogicalResourceRecord,
    phase: MigrationPhase,
) -> Result<(), MigrationOperationError> {
    if checkpoint.phase() != phase
        || checkpoint.project_id() != source.project_id()
        || checkpoint.source_compatibility_fingerprint() != source.compatibility_fingerprint()
    {
        return Err(MigrationOperationError::new(
            "MySQL-family source operation does not match its durable checkpoint",
        ));
    }
    Ok(())
}

fn validate_retirement(
    inventory: &MigrationRecord,
    checkpoint: &MigrationRecord,
    options: &MySqlMigrationOperationsOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let source = options.source_logical_resource;
    let database_name = options
        .source_environment
        .values()
        .get("DB_DATABASE")
        .map(String::as_str);
    let expected_kind = match options.flavor {
        MySqlFlavor::MySql => "mysql_database",
        MySqlFlavor::MariaDb => "mariadb_database",
    };
    let expected_service = match options.flavor {
        MySqlFlavor::MySql => "mysql",
        MySqlFlavor::MariaDb => "mariadb",
    };
    if inventory.phase() != MigrationPhase::Inventoried
        || checkpoint.phase() != MigrationPhase::Cutover
        || !checkpoint.has_same_identity(inventory)
        || checkpoint.rollback_reference() != inventory.rollback_reference()
        || source.kind() != expected_kind
        || source.lifecycle() != ResourceLifecycle::Active
        || source.project_id() != inventory.project_id()
        || source.compatibility_fingerprint() != inventory.source_compatibility_fingerprint()
        || database_name != Some(source.logical_resource_id())
        || !database_name.is_some_and(|database| valid_identifier(database, 64))
        || options.source_credential.project_id() != Some(source.project_id())
        || options.source_credential.service_id() != source.service_id()
        || !valid_identifier(options.source_credential.username(), 32)
        || options.source_credential.lifecycle() != CredentialLifecycle::Active
        || options.source_administrator.project_id().is_some()
        || options.source_administrator.service_id() != expected_service
        || options.source_administrator.username() != "root"
        || options.source_administrator.secret().is_empty()
        || options.source_administrator.lifecycle() != CredentialLifecycle::Active
        || options.source_environment.project_id() != source.project_id()
        || options.source_environment.lifecycle() != EnvironmentLifecycle::Active
        || options.source_container.metadata().installation_id() != options.installation_id
        || options
            .source_container
            .metadata()
            .compatibility_fingerprint()
            != source.compatibility_fingerprint()
    {
        return Err(MigrationOperationError::new(
            "MySQL-family source retirement requires an exact confirmed cutover",
        ));
    }
    Ok(())
}

fn valid_identifier(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value.bytes().enumerate().all(|(index, byte)| match byte {
            b'a'..=b'z' | b'_' => true,
            b'0'..=b'9' => index > 0,
            _ => false,
        })
}

fn retirement_sql(database_name: &str, username: &str) -> String {
    format!(
        "DROP DATABASE IF EXISTS `{database_name}`;\n\
         DROP USER IF EXISTS '{username}'@'%';\n"
    )
}

const fn client_executable(flavor: MySqlFlavor) -> &'static str {
    match flavor {
        MySqlFlavor::MySql => "mysql",
        MySqlFlavor::MariaDb => "mariadb",
    }
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
