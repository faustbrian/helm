use super::PostgresProvisionTargetOptions;
use crate::control_plane::engine::{CommandExecutor, OwnedContainer};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::shared_infrastructure::provision_postgres_logical_resource;
use crate::control_plane::state::{MigrationPhase, ResourceLifecycle};

/// Creates the deterministic v8 database and role without touching the source.
pub(crate) async fn provision_postgres_target(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    options: &PostgresProvisionTargetOptions<'_>,
) -> Result<String, MigrationOperationError> {
    validate(container, options)?;
    provision_postgres_logical_resource(executor, container, options.plan, options.administrator)
        .await
        .map_err(|error| {
            MigrationOperationError::new(format!("PostgreSQL target provisioning failed: {error}"))
        })?;

    Ok(options.plan.database_name().to_owned())
}

fn validate(
    container: &OwnedContainer,
    options: &PostgresProvisionTargetOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let checkpoint = options.checkpoint;
    let target = options.target_logical_resource;
    let expected_logical_id = format!("{}/{}", target.project_id(), target.service_id());
    let invalid = checkpoint.phase() != MigrationPhase::BackupVerified
        || options.installation_id.is_empty()
        || target.lifecycle() != ResourceLifecycle::Active
        || target.kind() != "postgres_database_and_role"
        || target.project_id() != checkpoint.project_id()
        || target.logical_resource_id() != expected_logical_id
        || target.compatibility_fingerprint() != checkpoint.target_compatibility_fingerprint()
        || options.plan.project_id() != target.project_id()
        || options.plan.service_id() != target.service_id()
        || container.metadata().installation_id() != options.installation_id
        || container.metadata().compatibility_fingerprint()
            != checkpoint.target_compatibility_fingerprint();
    if invalid {
        return Err(MigrationOperationError::new(
            "PostgreSQL target provisioning request does not match its durable checkpoint",
        ));
    }

    Ok(())
}
