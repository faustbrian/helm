use crate::control_plane::migration::{
    V7MigrationAdapterRegistry, V7RecreatedServiceTarget, register_v7_no_op_migration_adapters,
    register_v7_recreated_service_migration_adapter,
};
use crate::control_plane::state::{
    LogicalResourceRecord, ResourceLifecycle, ResourceRecord, ResourceRetention,
    V7MigrationExecutionRecord,
};

/// Binds no-op and normally reconciled v8 targets to one accepted execution.
pub(crate) fn register_accepted_v7_recreated_adapters(
    registry: &mut V7MigrationAdapterRegistry<'_>,
    execution: &V7MigrationExecutionRecord,
    resources: &[ResourceRecord],
    logical_resources: &[LogicalResourceRecord],
) -> Result<usize, String> {
    let mut registered = register_v7_no_op_migration_adapters(registry, execution)?;
    for checkpoint in execution
        .checkpoints()
        .iter()
        .filter(|checkpoint| is_recreation(checkpoint.adapter_kind()))
    {
        let service_id = checkpoint
            .adapter_id()
            .strip_prefix("service/")
            .filter(|service_id| !service_id.is_empty() && !service_id.contains('/'))
            .ok_or_else(|| {
                format!(
                    "recreated v7 adapter '{}' has no exact service identity",
                    checkpoint.adapter_id()
                )
            })?;
        let target = match checkpoint.adapter_kind() {
            "recreate-ephemeral" => V7RecreatedServiceTarget::Ephemeral,
            "recreate-project-workload" => V7RecreatedServiceTarget::Workload(one(
                workload_targets(execution, service_id, resources),
                service_id,
            )?),
            "recreate-stateless" => {
                stateless_target(execution, service_id, resources, logical_resources)?
            }
            _ => unreachable!("filtered recreation kind"),
        };
        register_v7_recreated_service_migration_adapter(registry, execution, service_id, target)?;
        registered += 1;
    }

    Ok(registered)
}

fn is_recreation(kind: &str) -> bool {
    matches!(
        kind,
        "recreate-project-workload" | "recreate-stateless" | "recreate-ephemeral"
    )
}

fn stateless_target(
    execution: &V7MigrationExecutionRecord,
    service_id: &str,
    resources: &[ResourceRecord],
    logical_resources: &[LogicalResourceRecord],
) -> Result<V7RecreatedServiceTarget, String> {
    let workloads = workload_targets(execution, service_id, resources);
    let logical = logical_resources
        .iter()
        .filter(|resource| {
            resource.project_id() == execution.project_id()
                && resource.service_id() == service_id
                && resource.lifecycle() == ResourceLifecycle::Active
        })
        .cloned()
        .collect::<Vec<_>>();
    match (workloads.as_slice(), logical.as_slice()) {
        ([workload], []) => Ok(V7RecreatedServiceTarget::Workload(workload.clone())),
        ([], [logical]) => Ok(V7RecreatedServiceTarget::Logical(logical.clone())),
        ([], []) => Err(format!(
            "recreated v7 service '{service_id}' active v8 target is missing"
        )),
        _ => Err(format!(
            "recreated v7 service '{service_id}' active v8 target is ambiguous"
        )),
    }
}

fn workload_targets(
    execution: &V7MigrationExecutionRecord,
    service_id: &str,
    resources: &[ResourceRecord],
) -> Vec<ResourceRecord> {
    resources
        .iter()
        .filter(|resource| {
            resource.project_id() == Some(execution.project_id())
                && resource.scope_id() == Some(service_id)
                && resource.lifecycle() == ResourceLifecycle::Active
                && resource.retention() == ResourceRetention::Disposable
                && resource.schema_version() == 8
        })
        .cloned()
        .collect()
}

fn one(values: Vec<ResourceRecord>, service_id: &str) -> Result<ResourceRecord, String> {
    match values.as_slice() {
        [value] => Ok(value.clone()),
        [] => Err(format!(
            "recreated v7 service '{service_id}' active v8 workload target is missing"
        )),
        _ => Err(format!(
            "recreated v7 service '{service_id}' active v8 workload target is ambiguous"
        )),
    }
}
