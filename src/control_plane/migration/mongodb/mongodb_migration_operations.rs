use super::{
    MongoDbBackupOptions, MongoDbMigrationOperationsOptions, MongoDbRestoreOptions,
    MongoDbVerifyTargetOptions, backup_mongodb_database, restore_mongodb_database,
    verify_mongodb_target,
};
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, run_attached_command,
};
use crate::control_plane::migration::{
    MigrationBackup, MigrationCutoverPlan, MigrationFuture, MigrationOperationError,
    MigrationOperations, MigrationRollbackPlan, MigrationTargetPlan,
};
use crate::control_plane::shared_infrastructure::provision_mongodb_logical_resource;
use crate::control_plane::state::{
    CredentialLifecycle, EnvironmentLifecycle, MigrationPhase, MigrationRecord, ResourceLifecycle,
};
use std::collections::BTreeMap;

/// MongoDB implementation of every reversible migration operation.
pub(crate) struct MongoDbMigrationOperations<'operation, E> {
    executor: &'operation E,
    options: MongoDbMigrationOperationsOptions<'operation>,
}

impl<'operation, E> MongoDbMigrationOperations<'operation, E>
where
    E: CommandExecutor + Sync,
{
    pub(crate) fn new(
        executor: &'operation E,
        options: MongoDbMigrationOperationsOptions<'operation>,
    ) -> Result<Self, MigrationOperationError> {
        validate(&options)?;

        Ok(Self { executor, options })
    }
}

impl<E> MigrationOperations for MongoDbMigrationOperations<'_, E>
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
        let options = MongoDbBackupOptions {
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
            backup_mongodb_database(executor, container, &options).await
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
        let plan = self.options.target_plan;
        let logical = self.options.target_logical_resource.clone();
        let credential = self.options.target_credential.clone();
        Box::pin(async move {
            valid?;
            provision_mongodb_logical_resource(executor, container, plan)
                .await
                .map_err(|error| operation_error("MongoDB target provisioning failed", error))?;
            MigrationTargetPlan::new(plan.database_name(), logical, credential)
        })
    }

    fn restore<'operation>(
        &'operation mut self,
        checkpoint: &'operation MigrationRecord,
        _backup_reference: &'operation str,
        target_resource_id: &'operation str,
    ) -> MigrationFuture<'operation, ()> {
        let options = MongoDbRestoreOptions {
            checkpoint,
            source_logical_resource: self.options.source_logical_resource,
            administrator: self.options.target_administrator,
            installation_id: self.options.installation_id,
            target_database_name: target_resource_id,
            verified_at_unix_seconds: self.options.operation_unix_seconds,
            timeout: self.options.timeout,
        };
        let executor = self.executor;
        let container = self.options.target_container;
        Box::pin(async move { restore_mongodb_database(executor, container, &options).await })
    }

    fn verify_target<'operation>(
        &'operation mut self,
        checkpoint: &'operation MigrationRecord,
        target_resource_id: &'operation str,
    ) -> MigrationFuture<'operation, ()> {
        let options = MongoDbVerifyTargetOptions {
            checkpoint,
            credential: self.options.target_credential,
            installation_id: self.options.installation_id,
            target_database_name: target_resource_id,
            timeout: self.options.timeout,
        };
        let executor = self.executor;
        let container = self.options.target_container;
        Box::pin(async move { verify_mongodb_target(executor, container, &options).await })
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
            && target_resource_id == self.options.target_plan.database_name()
            && checkpoint.rollback_reference() == Some(rollback_reference);
        Box::pin(async move {
            if !valid {
                return Err(MigrationOperationError::new(
                    "MongoDB cutover plan does not match its verified checkpoint",
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
                    "MongoDB rollback plan does not match a reversible checkpoint",
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
        let script = retirement_script(&self.options).map(|script| script.into_bytes());
        let request = CommandRequest::new(
            vec![
                "mongosh".to_owned(),
                "--quiet".to_owned(),
                "--nodb".to_owned(),
            ],
            BTreeMap::new(),
            None,
        )
        .map_err(|error| operation_error("MongoDB retirement request is invalid", error));
        let executor = self.executor;
        let container = self.options.source_container;
        let timeout = self.options.timeout;
        Box::pin(async move {
            validation?;
            let command = AttachedCommandOptions::new(
                request?,
                script?,
                "retire confirmed MongoDB source",
                timeout,
            )
            .map_err(|error| operation_error("MongoDB retirement request is invalid", error))?;
            run_attached_command(executor, container, &command)
                .await
                .map_err(|error| operation_error("MongoDB source retirement failed", error))
        })
    }
}

fn validate(
    options: &MongoDbMigrationOperationsOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let source = options.source_logical_resource;
    let target = options.target_logical_resource;
    let retained = options.rollback.retained_targets().iter().any(|candidate| {
        candidate.logical_resource_id() == target.logical_resource_id()
            && candidate.shared_resource_id() == target.shared_resource_id()
            && candidate.lifecycle() == ResourceLifecycle::Retained
    });
    let invalid = options.installation_id.is_empty()
        || !options.backup_root.is_absolute()
        || options.operation_unix_seconds < 0
        || options.timeout.is_zero()
        || source.kind() != "mongodb_database"
        || target.kind() != "mongodb_database"
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
        || options.target_plan.database_name() != source.logical_resource_id()
        || options.source_administrator.project_id().is_some()
        || options.source_administrator.service_id() != "mongodb"
        || options.source_administrator.lifecycle() != CredentialLifecycle::Active
        || options.target_administrator.project_id() != Some(target.project_id())
        || options.target_administrator.service_id() != "mongodb"
        || options.target_administrator.lifecycle() != CredentialLifecycle::Active
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
            "MongoDB migration operations do not describe one exact reversible resource",
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
            "MongoDB source operation does not match its durable checkpoint",
        ));
    }
    Ok(())
}

fn validate_retirement(
    inventory: &MigrationRecord,
    checkpoint: &MigrationRecord,
    options: &MongoDbMigrationOperationsOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let source = options.source_logical_resource;
    let database_name = options
        .source_environment
        .values()
        .get("MONGODB_DATABASE")
        .map(String::as_str);
    if inventory.phase() != MigrationPhase::Inventoried
        || checkpoint.phase() != MigrationPhase::Cutover
        || !checkpoint.has_same_identity(inventory)
        || checkpoint.rollback_reference() != inventory.rollback_reference()
        || source.kind() != "mongodb_database"
        || source.lifecycle() != ResourceLifecycle::Active
        || source.project_id() != inventory.project_id()
        || database_name != Some(source.logical_resource_id())
        || options.source_credential.project_id() != Some(source.project_id())
        || options.source_credential.service_id() != source.service_id()
        || options.source_credential.lifecycle() != CredentialLifecycle::Active
        || options.source_administrator.project_id().is_some()
        || options.source_administrator.service_id() != "mongodb"
        || options.source_administrator.username() != "stackctl_admin"
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
            "MongoDB source retirement requires an exact confirmed cutover",
        ));
    }
    Ok(())
}

fn retirement_script(
    options: &MongoDbMigrationOperationsOptions<'_>,
) -> Result<String, MigrationOperationError> {
    let database = json_string(options.source_logical_resource.logical_resource_id())?;
    let username = json_string(options.source_credential.username())?;
    let administrator = json_string(options.source_administrator.username())?;
    let secret = json_string(options.source_administrator.secret())?;

    Ok(format!(
        "try {{\n\
         const admin = connect(\"mongodb://127.0.0.1:27017/admin\");\n\
         if (!admin.auth({administrator}, {secret})) {{ throw new Error(\"admin authentication failed\"); }}\n\
         const target = admin.getSiblingDB({database});\n\
         if (target.getUser({username}) !== null) {{ target.dropUser({username}); }}\n\
         target.dropDatabase();\n\
         }} catch (error) {{ print(error); quit(1); }}\n"
    ))
}

fn json_string(value: &str) -> Result<String, MigrationOperationError> {
    serde_json::to_string(value)
        .map_err(|error| operation_error("MongoDB retirement value encoding failed", error))
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
