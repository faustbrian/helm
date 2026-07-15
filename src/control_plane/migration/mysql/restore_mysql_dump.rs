use super::MySqlDumpRestoreOptions;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, OwnedContainer,
    StreamingCommandOptions, run_attached_command, run_streaming_command,
};
use std::collections::BTreeMap;

/// Streams one explicit developer dump into its exact owned MySQL-family schema.
pub(crate) async fn restore_mysql_dump(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    options: &MySqlDumpRestoreOptions<'_>,
) -> Result<(), EngineError> {
    validate(options)?;
    if options.reset {
        reset_database(executor, container, options).await?;
    }
    let request = CommandRequest::new(
        vec![
            options.flavor.client_executable().to_owned(),
            "--protocol=socket".to_owned(),
            format!("--user={}", options.credential.username()),
            format!("--database={}", options.logical.schema_name()),
            "--binary-mode".to_owned(),
        ],
        BTreeMap::from([(
            "MYSQL_PWD".to_owned(),
            options.credential.secret().to_owned(),
        )]),
        None,
    )?;
    let command = StreamingCommandOptions::new(
        request,
        "restore explicit MySQL-family dump",
        options.timeout,
    )?;
    let mut input =
        tokio::fs::File::open(options.file)
            .await
            .map_err(|error| EngineError::Backend {
                detail: format!("database dump could not be opened: {error}"),
            })?;
    let mut output = tokio::io::sink();

    run_streaming_command(executor, container, &command, &mut input, &mut output).await
}

async fn reset_database(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    options: &MySqlDumpRestoreOptions<'_>,
) -> Result<(), EngineError> {
    let request = CommandRequest::new(
        vec![
            options.flavor.client_executable().to_owned(),
            "--protocol=socket".to_owned(),
            "--user=root".to_owned(),
            "--batch".to_owned(),
        ],
        BTreeMap::from([(
            "MYSQL_PWD".to_owned(),
            options.administrator.secret().to_owned(),
        )]),
        None,
    )?;
    let input = format!(
        "DROP DATABASE IF EXISTS `{}`;\n{}",
        options.logical.schema_name(),
        options.logical.stdin_sql()
    );
    let command = AttachedCommandOptions::new(
        request,
        input.into_bytes(),
        "reset MySQL-family database before explicit dump restore",
        options.timeout,
    )?;

    run_attached_command(executor, container, &command).await
}

fn validate(options: &MySqlDumpRestoreOptions<'_>) -> Result<(), EngineError> {
    if options.timeout.is_zero()
        || options.credential.username() != options.logical.username()
        || options.credential.secret().is_empty()
        || options.administrator.secret().is_empty()
    {
        return Err(EngineError::InvalidRequest {
            detail: "MySQL-family dump restore does not match owned credentials".to_owned(),
        });
    }

    Ok(())
}
