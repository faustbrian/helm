use super::{ProjectBackupExecutionOptions, ProjectBackupExecutionResult};
use crate::control_plane::engine::{
    CommandExecutor, ContainerDiscovery, ResourceKind, reconstruct_owned_container,
};
use crate::control_plane::migration::{PostgresBackupOptions, backup_postgres_database};
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};

/// Resolves exact live ownership and creates one verified logical recovery point.
pub(crate) async fn execute_queued_project_backup<E>(
    engine: E,
    options: ProjectBackupExecutionOptions,
) -> ProjectBackupExecutionResult
where
    E: CommandExecutor + ContainerDiscovery,
{
    let created_at_unix_seconds = options.created_at_unix_seconds;
    let outcome = execute(&engine, &options)
        .await
        .map_err(|error| error.to_string());

    ProjectBackupExecutionResult::new(options.operation, created_at_unix_seconds, outcome)
}

async fn execute<E>(
    engine: &E,
    options: &ProjectBackupExecutionOptions,
) -> Result<crate::control_plane::migration::MigrationBackup, String>
where
    E: CommandExecutor + ContainerDiscovery,
{
    let logical = options
        .logical_resource
        .as_ref()
        .map_err(ToString::to_string)?;
    let credential = options.credential.as_ref().map_err(ToString::to_string)?;
    validate_runtime_state(options, logical, credential)?;
    let observed = engine
        .discover_managed()
        .await
        .map_err(|error| error.to_string())?;
    let mut matches = observed
        .iter()
        .filter_map(|container| {
            reconstruct_owned_container(container, &options.installation_id, options.schema_version)
                .ok()
        })
        .filter(|container| {
            container.metadata().kind() == ResourceKind::SharedService
                && container.metadata().project_id().is_none()
                && container.metadata().compatibility_fingerprint()
                    == options.operation.compatibility_fingerprint()
        });
    let container = matches.next().ok_or_else(|| {
        format!(
            "project backup found no owned shared service for compatibility '{}'",
            options.operation.compatibility_fingerprint()
        )
    })?;
    if matches.next().is_some() {
        return Err(format!(
            "project backup found multiple owned shared services for compatibility '{}'",
            options.operation.compatibility_fingerprint()
        ));
    }
    let backup = PostgresBackupOptions {
        logical_resource: logical,
        credential,
        database_name: logical.logical_resource_id(),
        installation_id: &options.installation_id,
        created_at_unix_seconds: options.created_at_unix_seconds,
        backup_root: &options.backup_root,
        timeout: options.timeout,
    };

    backup_postgres_database(engine, &container, &backup)
        .await
        .map_err(|error| error.to_string())
}

fn validate_runtime_state(
    options: &ProjectBackupExecutionOptions,
    logical: &crate::control_plane::state::LogicalResourceRecord,
    credential: &crate::control_plane::state::CredentialRecord,
) -> Result<(), String> {
    let operation = &options.operation;
    let valid = logical.logical_resource_id() == operation.logical_resource_id()
        && logical.project_id() == operation.project_id()
        && logical.service_id() == operation.service_id()
        && logical.kind() == operation.kind()
        && logical.compatibility_fingerprint() == operation.compatibility_fingerprint()
        && logical.lifecycle() == ResourceLifecycle::Active
        && credential.project_id() == Some(operation.project_id())
        && credential.service_id() == operation.service_id()
        && credential.lifecycle() == CredentialLifecycle::Active
        && options.created_at_unix_seconds >= 0
        && options.schema_version > 0
        && !options.installation_id.is_empty();
    if !valid {
        return Err(
            "project backup intent does not match active logical ownership and credentials"
                .to_owned(),
        );
    }

    Ok(())
}
