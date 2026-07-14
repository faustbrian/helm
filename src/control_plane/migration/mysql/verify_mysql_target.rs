use super::MySqlVerifyTargetOptions;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, OwnedContainer,
    run_attached_command_capture,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::shared_infrastructure::MySqlFlavor;
use crate::control_plane::state::{CredentialLifecycle, MigrationPhase};
use std::collections::BTreeMap;

const CATALOG_QUERY: &str = "SELECT CONCAT(DATABASE(), '\\t', CURRENT_USER());";

/// Requires tenant-authenticated access to the exact restored schema.
pub(crate) async fn verify_mysql_target(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    options: &MySqlVerifyTargetOptions<'_>,
) -> Result<(), MigrationOperationError> {
    validate(container, options)?;
    let request = CommandRequest::new(
        vec![
            client_executable(options.flavor).to_owned(),
            "--protocol=socket".to_owned(),
            format!("--user={}", options.credential.username()),
            format!("--database={}", options.target_database_name),
            "--batch".to_owned(),
            "--skip-column-names".to_owned(),
            format!("--execute={CATALOG_QUERY}"),
        ],
        BTreeMap::from([(
            "MYSQL_PWD".to_owned(),
            options.credential.secret().to_owned(),
        )]),
        None,
    )
    .map_err(|error| {
        operation_error("MySQL-family target verification request is invalid", error)
    })?;
    let command = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "verify MySQL-family target catalog",
        options.timeout,
    )
    .map_err(|error| {
        operation_error("MySQL-family target verification request is invalid", error)
    })?;
    let output = run_attached_command_capture(executor, container, &command)
        .await
        .map_err(|error| operation_error("MySQL-family target verification failed", error))?;
    let expected = format!(
        "{}\t{}@%\n",
        options.target_database_name,
        options.credential.username()
    );
    if output != expected.as_bytes() {
        return Err(MigrationOperationError::new(
            "MySQL-family target verification returned unexpected evidence",
        ));
    }

    Ok(())
}

fn validate(
    container: &OwnedContainer,
    options: &MySqlVerifyTargetOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let checkpoint = options.checkpoint;
    let invalid = checkpoint.phase() != MigrationPhase::DataRestored
        || checkpoint.target_resource_id() != Some(options.target_database_name)
        || options.installation_id.is_empty()
        || options.target_database_name.is_empty()
        || options.credential.project_id() != Some(checkpoint.project_id())
        || options.credential.username().is_empty()
        || options.credential.secret().is_empty()
        || options.credential.lifecycle() != CredentialLifecycle::Active
        || options.timeout.is_zero()
        || container.metadata().installation_id() != options.installation_id
        || container.metadata().compatibility_fingerprint()
            != checkpoint.target_compatibility_fingerprint();
    if invalid {
        return Err(MigrationOperationError::new(
            "MySQL-family target verification does not match its durable checkpoint",
        ));
    }

    Ok(())
}

const fn client_executable(flavor: MySqlFlavor) -> &'static str {
    match flavor {
        MySqlFlavor::MySql => "mysql",
        MySqlFlavor::MariaDb => "mariadb",
    }
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
