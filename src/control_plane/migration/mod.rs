#[cfg(test)]
mod tests;

mod execute_recovery_point_restore;
mod migration_backup;
mod migration_cutover_plan;
mod migration_error;
mod migration_execution_result;
mod migration_operation_error;
mod migration_operations;
mod migration_rollback_plan;
mod migration_target_plan;
mod mysql;
mod postgres;
mod recovery_point_restore_options;
mod run_migration;

pub(crate) use execute_recovery_point_restore::execute_recovery_point_restore;
pub(crate) use migration_backup::MigrationBackup;
pub(crate) use migration_cutover_plan::MigrationCutoverPlan;
pub(crate) use migration_error::MigrationError;
pub(crate) use migration_execution_result::MigrationExecutionResult;
pub(crate) use migration_operation_error::MigrationOperationError;
pub(crate) use migration_operations::{MigrationFuture, MigrationOperations};
pub(crate) use migration_rollback_plan::MigrationRollbackPlan;
pub(crate) use migration_target_plan::MigrationTargetPlan;
pub(crate) use mysql::{
    MySqlBackupOptions, MySqlMigrationOperations, MySqlMigrationOperationsOptions,
    MySqlRestoreOptions, backup_mysql_database, restore_mysql_database,
};
pub(crate) use postgres::{
    EnginePostgresSourceRetirement, PostgresBackupOptions, PostgresMigrationOperations,
    PostgresMigrationOperationsOptions, PostgresSourceRetirementOptions, backup_postgres_database,
};
pub(crate) use recovery_point_restore_options::RecoveryPointRestoreOptions;
pub(crate) use run_migration::{confirm_migration, execute_migration, rollback_migration};
