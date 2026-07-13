#[cfg(test)]
mod tests;

mod migration_backup;
mod migration_error;
mod migration_execution_result;
mod migration_operation_error;
mod migration_operations;
mod postgres;
mod run_migration;

pub(crate) use migration_backup::MigrationBackup;
pub(crate) use migration_error::MigrationError;
pub(crate) use migration_execution_result::MigrationExecutionResult;
pub(crate) use migration_operation_error::MigrationOperationError;
pub(crate) use migration_operations::{MigrationFuture, MigrationOperations};
pub(crate) use run_migration::{confirm_migration, execute_migration, rollback_migration};
