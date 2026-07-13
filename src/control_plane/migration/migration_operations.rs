use super::{
    MigrationBackup, MigrationCutoverPlan, MigrationOperationError, MigrationRollbackPlan,
};
use crate::control_plane::state::MigrationRecord;
use std::future::Future;
use std::pin::Pin;

pub(crate) type MigrationFuture<'operation, T> =
    Pin<Box<dyn Future<Output = Result<T, MigrationOperationError>> + Send + 'operation>>;

/// Idempotent resource-specific operations used by the migration coordinator.
pub(crate) trait MigrationOperations {
    fn backup<'operation>(
        &'operation mut self,
        checkpoint: &'operation MigrationRecord,
    ) -> MigrationFuture<'operation, MigrationBackup>;

    fn provision_target<'operation>(
        &'operation mut self,
        checkpoint: &'operation MigrationRecord,
    ) -> MigrationFuture<'operation, String>;

    fn restore<'operation>(
        &'operation mut self,
        checkpoint: &'operation MigrationRecord,
        backup_reference: &'operation str,
        target_resource_id: &'operation str,
    ) -> MigrationFuture<'operation, ()>;

    fn verify_target<'operation>(
        &'operation mut self,
        checkpoint: &'operation MigrationRecord,
        target_resource_id: &'operation str,
    ) -> MigrationFuture<'operation, ()>;

    /// Produces exact desired state for an atomic, replay-safe cutover.
    fn plan_cutover<'operation>(
        &'operation mut self,
        checkpoint: &'operation MigrationRecord,
        target_resource_id: &'operation str,
        rollback_reference: &'operation str,
    ) -> MigrationFuture<'operation, MigrationCutoverPlan>;

    /// Produces retained desired state for an atomic, replay-safe rollback.
    fn plan_rollback<'operation>(
        &'operation mut self,
        inventory: &'operation MigrationRecord,
        checkpoint: &'operation MigrationRecord,
    ) -> MigrationFuture<'operation, MigrationRollbackPlan>;

    /// Retires the source only after explicit migration confirmation.
    fn retire_source<'operation>(
        &'operation mut self,
        inventory: &'operation MigrationRecord,
        checkpoint: &'operation MigrationRecord,
    ) -> MigrationFuture<'operation, ()>;
}
