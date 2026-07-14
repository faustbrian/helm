use crate::control_plane::migration::{MigrationCutoverPlan, MigrationRollbackPlan};

/// One daemon-owned project-wide transition for an accepted v7 execution.
pub(crate) enum AcceptedV7MigrationAction<'operation> {
    PrepareAndCutover {
        desired_state: &'operation MigrationCutoverPlan,
    },
    Confirm,
    Rollback {
        restored_state: &'operation MigrationRollbackPlan,
    },
}
