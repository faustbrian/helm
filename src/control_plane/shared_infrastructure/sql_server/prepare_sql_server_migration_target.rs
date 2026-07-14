use super::{
    SqlServerMigrationInstancePlanOptions, SqlServerMigrationPreparationOptions,
    SqlServerPreparationError, SqlServerSharedInstancePlan,
};
use crate::control_plane::shared_infrastructure::{
    CredentialEntropy, CredentialSecret, SharedInstancePlan, generate_credential_secret,
};
use crate::control_plane::state::{CredentialLifecycle, StateStore};

const SQLCMD_PATH: &str = "/opt/mssql-tools18/bin/sqlcmd";

/// Reserves one stable SA credential before target Engine mutation.
pub(crate) fn prepare_sql_server_migration_target<Store, Entropy>(
    store: &mut Store,
    shared: &SharedInstancePlan,
    entropy: &Entropy,
    options: SqlServerMigrationPreparationOptions<'_>,
) -> Result<SqlServerSharedInstancePlan, SqlServerPreparationError>
where
    Store: StateStore,
    Entropy: CredentialEntropy,
{
    let candidate = instance_plan(shared, options, strong_secret(entropy)?)?;
    let credential = store
        .insert_credential_if_absent(candidate.bootstrap_credential())
        .map_err(invalid)?;
    if credential.lifecycle() != CredentialLifecycle::Active {
        return Err(invalid(format!(
            "migration credential '{}' is disabled and requires explicit recovery",
            credential.credential_id()
        )));
    }

    instance_plan(
        shared,
        options,
        CredentialSecret::new(credential.secret().to_owned()),
    )
}

fn instance_plan(
    shared: &SharedInstancePlan,
    options: SqlServerMigrationPreparationOptions<'_>,
    bootstrap_secret: CredentialSecret,
) -> Result<SqlServerSharedInstancePlan, SqlServerPreparationError> {
    SqlServerSharedInstancePlan::new_migration_target(
        shared,
        SqlServerMigrationInstancePlanOptions {
            migration_id: options.migration_id.to_owned(),
            project_id: options.project_id.to_owned(),
            installation_id: options.installation_id.to_owned(),
            network_name: options.network_name.to_owned(),
            schema_version: options.schema_version,
            desired_revision: options.desired_revision.to_owned(),
            bootstrap_secret,
            accept_eula: true,
            sqlcmd_path: SQLCMD_PATH.to_owned(),
        },
    )
    .map_err(invalid)
}

fn strong_secret(
    entropy: &impl CredentialEntropy,
) -> Result<CredentialSecret, SqlServerPreparationError> {
    let generated = generate_credential_secret(entropy).map_err(invalid)?;
    Ok(CredentialSecret::new(format!("St1{}", generated.expose())))
}

fn invalid(error: impl std::fmt::Display) -> SqlServerPreparationError {
    SqlServerPreparationError::new(error.to_string())
}
