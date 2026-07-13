use super::PostgresVerifyTargetOptions;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, OwnedContainer,
    run_attached_command_capture,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::state::{CredentialLifecycle, MigrationPhase};
use std::collections::BTreeMap;

const CATALOG_QUERY: &str = "SELECT current_database() || E'\\t' || \
pg_get_userbyid(datdba) || E'\\t' || \
(SELECT count(*)::text FROM pg_index WHERE NOT indisvalid) || E'\\t' || \
(SELECT count(*)::text FROM pg_constraint WHERE NOT convalidated) \
FROM pg_database WHERE datname = current_database();";

/// Requires the restored database to have expected ownership and valid catalogs.
pub(crate) async fn verify_postgres_target(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    options: &PostgresVerifyTargetOptions<'_>,
) -> Result<(), MigrationOperationError> {
    validate(container, options)?;
    let request = CommandRequest::new(
        vec![
            "psql".to_owned(),
            "--no-psqlrc".to_owned(),
            "--set=ON_ERROR_STOP=1".to_owned(),
            "--tuples-only".to_owned(),
            "--no-align".to_owned(),
            format!("--username={}", options.credential.username()),
            format!("--dbname={}", options.target_database_name),
            format!("--command={CATALOG_QUERY}"),
        ],
        BTreeMap::from([(
            "PGPASSWORD".to_owned(),
            options.credential.secret().to_owned(),
        )]),
        None,
    )
    .map_err(|error| operation_error("PostgreSQL target verification request is invalid", error))?;
    let command = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "verify PostgreSQL target catalog",
        options.timeout,
    )
    .map_err(|error| operation_error("PostgreSQL target verification request is invalid", error))?;
    let output = run_attached_command_capture(executor, container, &command)
        .await
        .map_err(|error| operation_error("PostgreSQL target verification failed", error))?;
    let expected = format!(
        "{}\t{}\t0\t0\n",
        options.target_database_name, options.target_role_name
    );
    if output != expected.as_bytes() {
        return Err(MigrationOperationError::new(
            "PostgreSQL target catalog verification returned unexpected evidence",
        ));
    }

    Ok(())
}

fn validate(
    container: &OwnedContainer,
    options: &PostgresVerifyTargetOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let checkpoint = options.checkpoint;
    let invalid = checkpoint.phase() != MigrationPhase::DataRestored
        || checkpoint.target_resource_id() != Some(options.target_database_name)
        || options.installation_id.is_empty()
        || options.target_database_name.is_empty()
        || options.target_role_name.is_empty()
        || options.credential.project_id() != Some(checkpoint.project_id())
        || options.credential.username() != options.target_role_name
        || options.credential.secret().is_empty()
        || options.credential.lifecycle() != CredentialLifecycle::Active
        || options.timeout.is_zero()
        || container.metadata().installation_id() != options.installation_id
        || container.metadata().compatibility_fingerprint()
            != checkpoint.target_compatibility_fingerprint();
    if invalid {
        return Err(MigrationOperationError::new(
            "PostgreSQL target verification request does not match its durable checkpoint",
        ));
    }

    Ok(())
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
