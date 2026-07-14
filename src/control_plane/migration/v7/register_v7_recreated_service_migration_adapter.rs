use super::v7_recreated_service_migration_adapter::V7RecreatedServiceMigrationAdapter;
use super::{V7MigrationAdapterRegistry, V7MigrationAdapterTarget, V7RecreatedServiceTarget};
use crate::control_plane::state::{
    ResourceLifecycle, ResourceRetention, V7MigrationExecutionRecord,
};

/// Binds one recreation checkpoint to exact active v8 observed or logical state.
pub(crate) fn register_v7_recreated_service_migration_adapter(
    registry: &mut V7MigrationAdapterRegistry<'_>,
    execution: &V7MigrationExecutionRecord,
    service_id: &str,
    target: V7RecreatedServiceTarget,
) -> Result<(), String> {
    if service_id.is_empty() || service_id.contains('/') || service_id.contains('\0') {
        return Err("recreated v7 service id is invalid".to_owned());
    }
    let adapter_id = format!("service/{service_id}");
    let checkpoint = execution
        .checkpoints()
        .iter()
        .find(|checkpoint| checkpoint.adapter_id() == adapter_id)
        .ok_or_else(|| format!("recreated v7 service '{service_id}' has no selected checkpoint"))?;
    if checkpoint.requires_recovery() {
        return Err(format!(
            "recreated v7 service '{service_id}' cannot satisfy persistent recovery"
        ));
    }
    let target = target_reference(
        checkpoint.adapter_kind(),
        execution.project_id(),
        service_id,
        target,
    )?;
    registry.register(
        checkpoint.adapter_id(),
        checkpoint.adapter_kind(),
        Box::new(V7RecreatedServiceMigrationAdapter::new(target)),
    )
}

fn target_reference(
    adapter_kind: &str,
    project_id: &str,
    service_id: &str,
    target: V7RecreatedServiceTarget,
) -> Result<V7MigrationAdapterTarget, String> {
    match (adapter_kind, target) {
        (
            "recreate-project-workload" | "recreate-stateless",
            V7RecreatedServiceTarget::Workload(resource),
        ) if resource.project_id() == Some(project_id)
            && resource.scope_id() == Some(service_id)
            && resource.lifecycle() == ResourceLifecycle::Active
            && resource.retention() == ResourceRetention::Disposable
            && resource.schema_version() == 8 =>
        {
            V7MigrationAdapterTarget::resource(format!("resource:{}", resource.resource_id()))
        }
        ("recreate-stateless", V7RecreatedServiceTarget::Logical(resource))
            if resource.project_id() == project_id
                && resource.service_id() == service_id
                && resource.lifecycle() == ResourceLifecycle::Active =>
        {
            V7MigrationAdapterTarget::resource(format!(
                "logical-resource:{}",
                resource.logical_resource_id()
            ))
        }
        ("recreate-ephemeral", V7RecreatedServiceTarget::Ephemeral) => {
            Ok(V7MigrationAdapterTarget::NoExternalTarget)
        }
        ("recreate-project-workload" | "recreate-stateless" | "recreate-ephemeral", _) => {
            Err(format!(
                "recreated v7 service '{service_id}' target does not match selected kind '{adapter_kind}'"
            ))
        }
        _ => Err(format!(
            "v7 service '{service_id}' kind '{adapter_kind}' is not a recreation adapter"
        )),
    }
}
