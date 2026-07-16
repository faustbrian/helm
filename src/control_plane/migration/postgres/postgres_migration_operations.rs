use super::{
    PostgresBackupOptions, PostgresMigrationOperationsOptions, PostgresProvisionTargetOptions,
    PostgresRestoreOptions, PostgresSourceRetirement, PostgresVerifyTargetOptions,
    backup_postgres_database, provision_postgres_target, restore_postgres_database,
    verify_postgres_target,
};
use crate::control_plane::engine::CommandExecutor;
use crate::control_plane::migration::{
    MigrationBackup, MigrationCutoverPlan, MigrationFuture, MigrationOperationError,
    MigrationOperations, MigrationRollbackPlan, MigrationTargetPlan,
};
use crate::control_plane::state::{
    CredentialLifecycle, LogicalResourceRecord, MigrationPhase, MigrationRecord, ResourceLifecycle,
};

/// PostgreSQL implementation of every reversible migration operation.
pub(crate) struct PostgresMigrationOperations<'operation, E, R> {
    executor: &'operation E,
    retirement: &'operation mut R,
    options: PostgresMigrationOperationsOptions<'operation>,
}

impl<'operation, E, R> PostgresMigrationOperations<'operation, E, R>
where
    E: CommandExecutor + Sync,
    R: PostgresSourceRetirement + Send,
{
    pub(crate) fn new(
        executor: &'operation E,
        retirement: &'operation mut R,
        options: PostgresMigrationOperationsOptions<'operation>,
    ) -> Result<Self, MigrationOperationError> {
        validate(&options)?;

        Ok(Self {
            executor,
            retirement,
            options,
        })
    }
}

impl<E, R> MigrationOperations for PostgresMigrationOperations<'_, E, R>
where
    E: CommandExecutor + Sync,
    R: PostgresSourceRetirement + Send,
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
        let options = PostgresBackupOptions {
            logical_resource: self.options.source_logical_resource,
            credential: self.options.source_credential,
            database_name: self.options.source_database_name,
            installation_id: self.options.installation_id,
            created_at_unix_seconds: self.options.operation_unix_seconds,
            backup_root: self.options.backup_root,
            timeout: self.options.timeout,
        };
        let executor = self.executor;
        let container = self.options.source_container;
        Box::pin(async move {
            validation?;
            backup_postgres_database(executor, container, &options).await
        })
    }

    fn provision_target<'operation>(
        &'operation mut self,
        checkpoint: &'operation MigrationRecord,
    ) -> MigrationFuture<'operation, MigrationTargetPlan> {
        let options = PostgresProvisionTargetOptions {
            checkpoint,
            target_logical_resource: self.options.target_logical_resource,
            plan: self.options.target_plan,
            administrator: self.options.administrator,
            installation_id: self.options.installation_id,
        };
        let executor = self.executor;
        let container = self.options.target_container;
        let logical_resource = self.options.target_logical_resource.clone();
        let credential = self.options.target_credential.clone();
        Box::pin(async move {
            let target_resource_id =
                provision_postgres_target(executor, container, &options).await?;
            MigrationTargetPlan::new(target_resource_id, logical_resource, credential)
        })
    }

    fn restore<'operation>(
        &'operation mut self,
        checkpoint: &'operation MigrationRecord,
        _backup_reference: &'operation str,
        target_resource_id: &'operation str,
    ) -> MigrationFuture<'operation, ()> {
        let options = PostgresRestoreOptions {
            checkpoint,
            source_logical_resource: self.options.source_logical_resource,
            credential: self.options.target_credential,
            installation_id: self.options.installation_id,
            target_database_name: target_resource_id,
            target_role_name: self.options.target_plan.role_name(),
            verified_at_unix_seconds: self.options.operation_unix_seconds,
            timeout: self.options.timeout,
        };
        let executor = self.executor;
        let container = self.options.target_container;
        Box::pin(async move { restore_postgres_database(executor, container, &options).await })
    }

    fn verify_target<'operation>(
        &'operation mut self,
        checkpoint: &'operation MigrationRecord,
        target_resource_id: &'operation str,
    ) -> MigrationFuture<'operation, ()> {
        let options = PostgresVerifyTargetOptions {
            checkpoint,
            credential: self.options.target_credential,
            installation_id: self.options.installation_id,
            target_database_name: target_resource_id,
            target_role_name: self.options.target_plan.role_name(),
            timeout: self.options.timeout,
        };
        let executor = self.executor;
        let container = self.options.target_container;
        Box::pin(async move { verify_postgres_target(executor, container, &options).await })
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
                    "PostgreSQL cutover plan does not match its verified checkpoint",
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
                    "PostgreSQL rollback plan does not match a reversible checkpoint",
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
        if inventory.phase() != MigrationPhase::Inventoried
            || checkpoint.phase() != MigrationPhase::Cutover
            || !checkpoint.has_same_identity(inventory)
            || checkpoint.source_compatibility_fingerprint()
                != self
                    .options
                    .source_logical_resource
                    .compatibility_fingerprint()
        {
            return Box::pin(async {
                Err(MigrationOperationError::new(
                    "PostgreSQL source retirement requires an exact cutover checkpoint",
                ))
            });
        }
        self.retirement
            .retire_source(inventory, checkpoint, self.options.source_logical_resource)
    }
}

fn validate(
    options: &PostgresMigrationOperationsOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let source = options.source_logical_resource;
    let target = options.target_logical_resource;
    let target_retained = options.rollback.retained_targets().iter().any(|retained| {
        retained.logical_resource_id() == target.logical_resource_id()
            && retained.shared_resource_id() == target.shared_resource_id()
            && retained.project_id() == target.project_id()
            && retained.service_id() == target.service_id()
            && retained.kind() == target.kind()
            && retained.compatibility_fingerprint() == target.compatibility_fingerprint()
            && retained.lifecycle() == ResourceLifecycle::Retained
    });
    let invalid = options.installation_id.is_empty()
        || options.source_database_name.is_empty()
        || !options.backup_root.is_absolute()
        || options.operation_unix_seconds < 0
        || options.timeout.is_zero()
        || source.kind() != "postgres_database_and_role"
        || target.kind() != "postgres_database_and_role"
        || !matches!(
            source.lifecycle(),
            ResourceLifecycle::Active | ResourceLifecycle::Retained
        )
        || target.lifecycle() != ResourceLifecycle::Active
        || source.project_id() != target.project_id()
        || source.service_id() != target.service_id()
        || options.source_credential.project_id() != Some(source.project_id())
        || options.source_credential.service_id() != source.service_id()
        || options.source_credential.lifecycle() != CredentialLifecycle::Active
        || options.target_credential.project_id() != Some(target.project_id())
        || options.target_credential.service_id() != target.service_id()
        || !options
            .target_plan
            .matches_credential(options.target_credential)
        || options.target_credential.lifecycle() != CredentialLifecycle::Active
        || options.target_plan.project_id() != target.project_id()
        || options.target_plan.service_id() != target.service_id()
        || options.cutover.project().project_name() != target.project_id()
        || options.rollback.project().project_name() != target.project_id()
        || !target_retained
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
            "PostgreSQL migration operations do not describe one exact reversible resource",
        ));
    }

    Ok(())
}

fn validate_source_checkpoint(
    checkpoint: &MigrationRecord,
    source: &LogicalResourceRecord,
    phase: MigrationPhase,
) -> Result<(), MigrationOperationError> {
    if checkpoint.phase() != phase
        || checkpoint.project_id() != source.project_id()
        || checkpoint.source_compatibility_fingerprint() != source.compatibility_fingerprint()
    {
        return Err(MigrationOperationError::new(
            "PostgreSQL source operation does not match its durable checkpoint",
        ));
    }

    Ok(())
}
