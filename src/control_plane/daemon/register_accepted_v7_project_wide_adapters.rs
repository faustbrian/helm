use super::RegisterAcceptedV7ProjectWideAdaptersOptions;
use crate::control_plane::migration::{
    V7MigrationAdapterRegistry, V7ProtectedGeneratedEnvironmentAdapterOptions,
    register_v7_gateway_snapshot_migration_adapter,
    register_v7_installation_trust_migration_adapter,
    register_v7_protected_environment_migration_adapter,
};
use crate::control_plane::state::{
    EnvironmentLifecycle, ManagedEnvironmentRecord, V7MigrationExecutionRecord,
};

/// Binds selected gateway, trust, and generated-environment checkpoints.
pub(crate) fn register_accepted_v7_project_wide_adapters<'operation>(
    registry: &mut V7MigrationAdapterRegistry<'operation>,
    execution: &V7MigrationExecutionRecord,
    options: RegisterAcceptedV7ProjectWideAdaptersOptions<'operation>,
) -> Result<usize, String> {
    if options.accepted.project_id() != execution.project_id()
        || options.accepted.canonical_project_path() != execution.canonical_project_path()
        || options.accepted.evidence_revision() != execution.evidence_revision()
    {
        return Err("accepted v7 project-wide registration identity is inconsistent".to_owned());
    }
    let mut registered = 0;
    if selected(execution, "route", "gateway-snapshot-cutover") {
        let gateway = options.gateway.ok_or_else(|| {
            "accepted v7 route migration has no complete gateway snapshots".to_owned()
        })?;
        if register_v7_gateway_snapshot_migration_adapter(registry, execution, gateway)? {
            registered += 1;
        }
    }
    if selected(
        execution,
        "trust",
        "installation-legacy-caddy-ca-transition",
    ) {
        let trust = options.trust.ok_or_else(|| {
            "accepted v7 trust migration has no installation trust capability".to_owned()
        })?;
        if register_v7_installation_trust_migration_adapter(registry, execution, trust)? {
            registered += 1;
        }
    }
    if selected(execution, "environment", "protected-generated-environment") {
        let target = one_environment(execution, options.managed_environments)?;
        if register_v7_protected_environment_migration_adapter(
            registry,
            execution,
            V7ProtectedGeneratedEnvironmentAdapterOptions {
                accepted: options.accepted,
                target_environment: target,
                verified_at_unix_seconds: options.verified_at_unix_seconds,
                maximum_environment_bytes: options.maximum_environment_bytes,
            },
        )? {
            registered += 1;
        }
    }

    Ok(registered)
}

fn selected(execution: &V7MigrationExecutionRecord, adapter_id: &str, kind: &str) -> bool {
    execution.checkpoints().iter().any(|checkpoint| {
        checkpoint.adapter_id() == adapter_id && checkpoint.adapter_kind() == kind
    })
}

fn one_environment<'operation>(
    execution: &V7MigrationExecutionRecord,
    environments: &'operation [ManagedEnvironmentRecord],
) -> Result<&'operation ManagedEnvironmentRecord, String> {
    let matching = environments
        .iter()
        .filter(|environment| {
            environment.project_id() == execution.project_id()
                && environment.lifecycle() == EnvironmentLifecycle::Active
        })
        .collect::<Vec<_>>();
    match matching.as_slice() {
        [environment] => Ok(*environment),
        [] => Err(format!(
            "accepted v7 project '{}' active managed environment is missing",
            execution.project_id()
        )),
        _ => Err(format!(
            "accepted v7 project '{}' active managed environment is ambiguous",
            execution.project_id()
        )),
    }
}
