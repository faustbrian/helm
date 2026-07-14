use crate::control_plane::application::ControlPlane;
use crate::control_plane::migration::{
    V7MigrationAdapterPlan, V7MigrationExecutionPlanOptions, plan_v7_migration_execution,
};
use crate::control_plane::state::{
    AcceptedV7InventoryRecord, StateStore, V7MigrationExecutionRecord,
};

/// Binds accepted inventory to its immutable durable adapter checkpoint set.
pub(crate) fn record_accepted_v7_migration_plan<Store>(
    control_plane: &mut ControlPlane<Store>,
    accepted: &AcceptedV7InventoryRecord,
    adapter_plan: &V7MigrationAdapterPlan,
) -> Result<V7MigrationExecutionRecord, String>
where
    Store: StateStore,
{
    if adapter_plan.evidence_revision() != accepted.evidence_revision() {
        return Err("v7 adapter plan differs from accepted source evidence".to_owned());
    }
    let planned = plan_v7_migration_execution(V7MigrationExecutionPlanOptions {
        project_id: accepted.project_id(),
        canonical_project_path: accepted.canonical_project_path(),
        adapter_plan,
        planned_at_unix_seconds: accepted.accepted_at_unix_seconds(),
    })?;
    control_plane
        .record_v7_migration_execution(&planned)
        .map_err(|error| error.to_string())?;
    let persisted = control_plane
        .v7_migration_execution(
            accepted.canonical_project_path(),
            accepted.evidence_revision(),
        )
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "durable v7 migration plan disappeared after persistence".to_owned())?;
    if persisted != planned {
        return Err("durable v7 migration plan differs from accepted adapter plan".to_owned());
    }

    Ok(persisted)
}
