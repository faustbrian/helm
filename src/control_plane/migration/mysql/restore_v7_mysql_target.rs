use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, OwnedContainer,
    StreamingCommandOptions, run_attached_command, run_streaming_command,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::retention::StoredBackupArtifact;
use crate::control_plane::shared_infrastructure::{MySqlFlavor, MySqlLogicalResourcePlan};
use crate::control_plane::state::{CredentialLifecycle, CredentialRecord};
use std::collections::BTreeMap;
use std::time::Duration;

/// Resets and restores a deterministic v8 schema so preparation is replay-safe.
pub(super) async fn restore_v7_mysql_target(
    executor: &(impl CommandExecutor + Sync),
    container: &OwnedContainer,
    flavor: MySqlFlavor,
    plan: &MySqlLogicalResourcePlan,
    administrator: &CredentialRecord,
    target_credential: &CredentialRecord,
    stored: &StoredBackupArtifact,
    timeout: Duration,
) -> Result<(), MigrationOperationError> {
    reset_and_provision_target(executor, container, flavor, plan, administrator, timeout).await?;
    let request = CommandRequest::new(
        vec![
            client_executable(flavor).to_owned(),
            "--protocol=socket".to_owned(),
            format!("--user={}", target_credential.username()),
            format!("--database={}", plan.schema_name()),
            "--binary-mode".to_owned(),
        ],
        BTreeMap::from([(
            "MYSQL_PWD".to_owned(),
            target_credential.secret().to_owned(),
        )]),
        None,
    )
    .map_err(|error| operation_error("v8 MySQL-family restore request is invalid", error))?;
    let command = StreamingCommandOptions::new(
        request,
        "restore v7 data into v8 MySQL-family target",
        timeout,
    )
    .map_err(|error| operation_error("v8 MySQL-family restore request is invalid", error))?;
    let mut artifact = tokio::fs::File::open(stored.artifact_file())
        .await
        .map_err(|error| operation_error("open v7 MySQL-family restore artifact", error))?;
    let mut output = tokio::io::sink();
    run_streaming_command(executor, container, &command, &mut artifact, &mut output)
        .await
        .map_err(|error| operation_error("restore v8 MySQL-family migration target", error))
}

async fn reset_and_provision_target(
    executor: &(impl CommandExecutor + Sync),
    container: &OwnedContainer,
    flavor: MySqlFlavor,
    plan: &MySqlLogicalResourcePlan,
    administrator: &CredentialRecord,
    timeout: Duration,
) -> Result<(), MigrationOperationError> {
    if administrator.lifecycle() != CredentialLifecycle::Active
        || administrator.username() != "root"
        || administrator.secret().is_empty()
    {
        return Err(MigrationOperationError::new(
            "v8 MySQL-family target administrator is not active root",
        ));
    }
    let request = CommandRequest::new(
        vec![
            client_executable(flavor).to_owned(),
            "--protocol=socket".to_owned(),
            "--user=root".to_owned(),
            "--batch".to_owned(),
            "--skip-column-names".to_owned(),
        ],
        BTreeMap::from([("MYSQL_PWD".to_owned(), administrator.secret().to_owned())]),
        None,
    )
    .map_err(|error| operation_error("v8 MySQL-family target reset is invalid", error))?;
    let input = format!(
        "DROP DATABASE IF EXISTS `{}`;\n{}",
        plan.schema_name(),
        plan.stdin_sql()
    );
    let command = AttachedCommandOptions::new(
        request,
        input.into_bytes(),
        "reset v8 MySQL-family migration target",
        timeout,
    )
    .map_err(|error| operation_error("v8 MySQL-family target reset is invalid", error))?;
    run_attached_command(executor, container, &command)
        .await
        .map_err(|error| operation_error("reset v8 MySQL-family migration target", error))
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
