use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, OwnedContainer,
    StreamingCommandOptions, run_attached_command, run_streaming_command,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::retention::StoredBackupArtifact;
use crate::control_plane::shared_infrastructure::{
    PostgresLogicalResourcePlan, provision_postgres_logical_resource,
};
use crate::control_plane::state::CredentialRecord;
use std::collections::BTreeMap;
use std::time::Duration;

/// Resets and restores one deterministic v8 target so preparation is replay-safe.
pub(super) async fn restore_v7_postgres_target(
    executor: &(impl CommandExecutor + Sync),
    container: &OwnedContainer,
    plan: &PostgresLogicalResourcePlan,
    administrator: &CredentialRecord,
    target_credential: &CredentialRecord,
    stored: &StoredBackupArtifact,
    timeout: Duration,
) -> Result<(), MigrationOperationError> {
    provision_postgres_logical_resource(executor, container, plan, administrator)
        .await
        .map_err(|error| operation_error("provision v8 PostgreSQL target", error))?;
    reset_target(executor, container, plan, administrator, timeout).await?;

    let request = CommandRequest::new(
        vec![
            "pg_restore".to_owned(),
            "--exit-on-error".to_owned(),
            "--single-transaction".to_owned(),
            "--no-owner".to_owned(),
            "--no-privileges".to_owned(),
            format!("--username={}", target_credential.username()),
            format!("--dbname={}", plan.database_name()),
        ],
        BTreeMap::from([(
            "PGPASSWORD".to_owned(),
            target_credential.secret().to_owned(),
        )]),
        None,
    )
    .map_err(|error| operation_error("v8 PostgreSQL restore request is invalid", error))?;
    let command = StreamingCommandOptions::new(
        request,
        "restore v7 data into v8 PostgreSQL target",
        timeout,
    )
    .map_err(|error| operation_error("v8 PostgreSQL restore request is invalid", error))?;
    let mut artifact = tokio::fs::File::open(stored.artifact_file())
        .await
        .map_err(|error| operation_error("open v7 PostgreSQL restore artifact", error))?;
    let mut output = tokio::io::sink();
    run_streaming_command(executor, container, &command, &mut artifact, &mut output)
        .await
        .map_err(|error| operation_error("restore v8 PostgreSQL migration target", error))
}

async fn reset_target(
    executor: &(impl CommandExecutor + Sync),
    container: &OwnedContainer,
    plan: &PostgresLogicalResourcePlan,
    administrator: &CredentialRecord,
    timeout: Duration,
) -> Result<(), MigrationOperationError> {
    let request = CommandRequest::new(
        vec![
            "psql".to_owned(),
            "--no-psqlrc".to_owned(),
            "--set=ON_ERROR_STOP=1".to_owned(),
            format!("--username={}", administrator.username()),
            "--dbname=postgres".to_owned(),
        ],
        BTreeMap::from([("PGPASSWORD".to_owned(), administrator.secret().to_owned())]),
        None,
    )
    .map_err(|error| operation_error("v8 PostgreSQL target reset request is invalid", error))?;
    let command = AttachedCommandOptions::new(
        request,
        reset_sql(plan.database_name(), plan.role_name()).into_bytes(),
        "reset v8 PostgreSQL migration target",
        timeout,
    )
    .map_err(|error| operation_error("v8 PostgreSQL target reset request is invalid", error))?;
    run_attached_command(executor, container, &command)
        .await
        .map_err(|error| operation_error("reset v8 PostgreSQL migration target", error))
}

fn reset_sql(database_name: &str, role_name: &str) -> String {
    format!(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity \
         WHERE datname = '{database_name}' AND pid <> pg_backend_pid();\n\
         DROP DATABASE IF EXISTS {database_name};\n\
         CREATE DATABASE {database_name} OWNER {role_name};\n\
         REVOKE ALL ON DATABASE {database_name} FROM PUBLIC;\n\
         GRANT CONNECT, TEMPORARY ON DATABASE {database_name} TO {role_name};\n"
    )
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
