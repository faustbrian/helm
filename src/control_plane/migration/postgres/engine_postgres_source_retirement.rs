use super::{PostgresSourceRetirement, PostgresSourceRetirementOptions};
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, run_attached_command,
};
use crate::control_plane::migration::{MigrationFuture, MigrationOperationError};
use crate::control_plane::state::{
    CredentialLifecycle, EnvironmentLifecycle, LogicalResourceRecord, MigrationPhase,
    MigrationRecord, ResourceLifecycle,
};
use std::collections::BTreeMap;

const POSTGRES_BOOTSTRAP_USERNAME: &str = "stackctl_admin";

/// Direct-Engine confirmed retirement of one PostgreSQL database and role.
pub(crate) struct EnginePostgresSourceRetirement<'operation, E> {
    executor: &'operation E,
    options: PostgresSourceRetirementOptions<'operation>,
}

impl<'operation, E> EnginePostgresSourceRetirement<'operation, E>
where
    E: CommandExecutor + Sync,
{
    pub(crate) fn new(
        executor: &'operation E,
        options: PostgresSourceRetirementOptions<'operation>,
    ) -> Result<Self, MigrationOperationError> {
        let source_database_name = source_database_name(&options)?;
        let source_role_name = options.source_credential.username();
        if options.installation_id.is_empty()
            || options.timeout.is_zero()
            || !valid_identifier(source_database_name)
            || !valid_identifier(source_role_name)
            || options.administrator.username() != POSTGRES_BOOTSTRAP_USERNAME
            || options.administrator.secret().is_empty()
            || options.administrator.lifecycle() != CredentialLifecycle::Active
            || options.source_credential.lifecycle() != CredentialLifecycle::Active
            || options.source_environment.lifecycle() != EnvironmentLifecycle::Active
            || options.source_container.metadata().installation_id() != options.installation_id
        {
            return Err(MigrationOperationError::new(
                "PostgreSQL source retirement options are not exact and owned",
            ));
        }

        Ok(Self { executor, options })
    }
}

impl<E> PostgresSourceRetirement for EnginePostgresSourceRetirement<'_, E>
where
    E: CommandExecutor + Sync,
{
    fn retire_source<'operation>(
        &'operation mut self,
        inventory: &'operation MigrationRecord,
        checkpoint: &'operation MigrationRecord,
        source: &'operation LogicalResourceRecord,
    ) -> MigrationFuture<'operation, ()> {
        let validation = validate_source(inventory, checkpoint, source, &self.options);
        let request = CommandRequest::new(
            vec![
                "psql".to_owned(),
                "--no-psqlrc".to_owned(),
                "--set=ON_ERROR_STOP=1".to_owned(),
                format!("--username={POSTGRES_BOOTSTRAP_USERNAME}"),
                "--dbname=postgres".to_owned(),
            ],
            BTreeMap::from([(
                "PGPASSWORD".to_owned(),
                self.options.administrator.secret().to_owned(),
            )]),
            None,
        )
        .map_err(|error| operation_error("PostgreSQL retirement request is invalid", error));
        let source_database_name = source_database_name(&self.options);
        let source_role_name = self.options.source_credential.username().to_owned();
        let executor = self.executor;
        let container = self.options.source_container;
        let timeout = self.options.timeout;

        Box::pin(async move {
            validation?;
            let source_database_name = source_database_name?;
            let request = request?;
            let command = AttachedCommandOptions::new(
                request,
                retirement_sql(source_database_name, &source_role_name).into_bytes(),
                "retire confirmed PostgreSQL source",
                timeout,
            )
            .map_err(|error| operation_error("PostgreSQL retirement request is invalid", error))?;
            run_attached_command(executor, container, &command)
                .await
                .map_err(|error| operation_error("PostgreSQL source retirement failed", error))
        })
    }
}

fn validate_source(
    inventory: &MigrationRecord,
    checkpoint: &MigrationRecord,
    source: &LogicalResourceRecord,
    options: &PostgresSourceRetirementOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let invalid = inventory.phase() != MigrationPhase::Inventoried
        || checkpoint.phase() != MigrationPhase::Cutover
        || !checkpoint.has_same_identity(inventory)
        || checkpoint.rollback_reference() != inventory.rollback_reference()
        || source.kind() != "postgres_database_and_role"
        || source.lifecycle() != ResourceLifecycle::Active
        || source.project_id() != inventory.project_id()
        || source.compatibility_fingerprint() != inventory.source_compatibility_fingerprint()
        || options.source_environment.project_id() != source.project_id()
        || options.source_credential.project_id() != Some(source.project_id())
        || options.source_credential.service_id() != source.service_id()
        || options
            .source_container
            .metadata()
            .compatibility_fingerprint()
            != source.compatibility_fingerprint();
    if invalid {
        return Err(MigrationOperationError::new(
            "PostgreSQL source retirement does not match the confirmed cutover source",
        ));
    }

    Ok(())
}

fn source_database_name<'operation>(
    options: &'operation PostgresSourceRetirementOptions<'_>,
) -> Result<&'operation str, MigrationOperationError> {
    options
        .source_environment
        .values()
        .get("DB_DATABASE")
        .map(String::as_str)
        .ok_or_else(|| {
            MigrationOperationError::new(
                "PostgreSQL source environment does not contain DB_DATABASE",
            )
        })
}

fn retirement_sql(database_name: &str, role_name: &str) -> String {
    format!(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity \
         WHERE datname = '{database_name}' AND pid <> pg_backend_pid();\n\
         DROP DATABASE IF EXISTS {database_name};\n\
         DROP ROLE IF EXISTS {role_name};\n"
    )
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && value.bytes().enumerate().all(|(index, byte)| match byte {
            b'a'..=b'z' | b'_' => true,
            b'0'..=b'9' => index > 0,
            _ => false,
        })
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
