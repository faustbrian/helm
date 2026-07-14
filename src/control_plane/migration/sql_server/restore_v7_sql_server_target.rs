use crate::control_plane::engine::{
    CommandExecutor, CommandRequest, OwnedContainer, StreamingCommandOptions, run_streaming_command,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::retention::StoredBackupArtifact;
use crate::control_plane::shared_infrastructure::SqlServerLogicalResourcePlan;
use crate::control_plane::state::{CredentialLifecycle, CredentialRecord};
use std::collections::BTreeMap;
use std::time::Duration;

const SQLCMD_PATH: &str = "/opt/mssql-tools18/bin/sqlcmd";
const RESTORE_SCRIPT: &str = "set -eu\n\
    trap 'rm -f \"$STACKCTL_RESTORE_FILE\"' EXIT HUP INT TERM\n\
    cat > \"$STACKCTL_RESTORE_FILE\"\n\
    \"$STACKCTL_SQLCMD\" -b -C -S 127.0.0.1 -U sa -d master -Q \"$STACKCTL_VERIFY_SQL\" -o /dev/null\n\
    \"$STACKCTL_SQLCMD\" -b -C -S 127.0.0.1 -U sa -d master -Q \"$STACKCTL_RESET_SQL\" -o /dev/null\n\
    \"$STACKCTL_SQLCMD\" -b -C -S 127.0.0.1 -U sa -d master -Q \"$STACKCTL_RESTORE_SQL\" -o /dev/null\n\
    \"$STACKCTL_SQLCMD\" -b -C -S 127.0.0.1 -U sa -d master -Q \"$STACKCTL_PROVISION_SQL\" -o /dev/null";

/// Recreates one deterministic v8 SQL Server database from verified recovery.
pub(super) async fn restore_v7_sql_server_target(
    executor: &(impl CommandExecutor + Sync),
    container: &OwnedContainer,
    plan: &SqlServerLogicalResourcePlan,
    administrator: &CredentialRecord,
    target_credential: &CredentialRecord,
    stored: &StoredBackupArtifact,
    timeout: Duration,
) -> Result<(), MigrationOperationError> {
    validate(container, plan, administrator, target_credential)?;
    let restore_file = format!(
        "/var/opt/mssql/data/.stackctl-v7-restore-{}.bak",
        container.metadata().resource_id().unwrap_or_default()
    );
    let database = plan.database_name();
    let login = plan.username();
    let reset_sql = format!(
        "IF DB_ID(N'{database}') IS NOT NULL BEGIN ALTER DATABASE [{database}] SET SINGLE_USER WITH ROLLBACK IMMEDIATE; DROP DATABASE [{database}]; END; IF SUSER_ID(N'{login}') IS NOT NULL DROP LOGIN [{login}];"
    );
    let verify_sql = format!("RESTORE VERIFYONLY FROM DISK = N'{restore_file}' WITH CHECKSUM");
    let restore_sql = format!(
        "RESTORE DATABASE [{database}] FROM DISK = N'{restore_file}' WITH REPLACE, RECOVERY, CHECKSUM"
    );
    let request = CommandRequest::new(
        vec!["sh".to_owned(), "-c".to_owned(), RESTORE_SCRIPT.to_owned()],
        BTreeMap::from([
            (
                "SQLCMDPASSWORD".to_owned(),
                administrator.secret().to_owned(),
            ),
            (
                "STACKCTL_PROVISION_SQL".to_owned(),
                plan.stdin_sql().to_owned(),
            ),
            ("STACKCTL_RESET_SQL".to_owned(), reset_sql),
            ("STACKCTL_RESTORE_FILE".to_owned(), restore_file),
            ("STACKCTL_RESTORE_SQL".to_owned(), restore_sql),
            ("STACKCTL_SQLCMD".to_owned(), SQLCMD_PATH.to_owned()),
            ("STACKCTL_VERIFY_SQL".to_owned(), verify_sql),
        ]),
        None,
    )
    .map_err(|error| operation_error("v8 SQL Server restore request is invalid", error))?;
    let command = StreamingCommandOptions::new(
        request,
        "restore v7 data into v8 SQL Server target",
        timeout,
    )
    .map_err(|error| operation_error("v8 SQL Server restore request is invalid", error))?;
    let mut artifact = tokio::fs::File::open(stored.artifact_file())
        .await
        .map_err(|error| operation_error("open v7 SQL Server restore artifact", error))?;
    let mut output = tokio::io::sink();
    run_streaming_command(executor, container, &command, &mut artifact, &mut output)
        .await
        .map_err(|error| operation_error("restore v8 SQL Server migration target", error))
}

fn validate(
    container: &OwnedContainer,
    plan: &SqlServerLogicalResourcePlan,
    administrator: &CredentialRecord,
    target_credential: &CredentialRecord,
) -> Result<(), MigrationOperationError> {
    let expected_administrator = format!(
        "migration/{}/sqlserver-bootstrap",
        container.metadata().resource_id().unwrap_or_default()
    );
    let invalid = administrator.credential_id() != expected_administrator
        || administrator.username() != "sa"
        || administrator.secret().is_empty()
        || administrator.lifecycle() != CredentialLifecycle::Active
        || target_credential.lifecycle() != CredentialLifecycle::Active
        || !plan.matches_credential(target_credential);
    if invalid {
        return Err(MigrationOperationError::new(
            "v8 SQL Server restore credentials differ from the deterministic target",
        ));
    }
    Ok(())
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
