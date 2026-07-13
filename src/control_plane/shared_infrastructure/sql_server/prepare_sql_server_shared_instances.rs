use super::{
    PreparedSqlServerSharedInstance, SqlServerPreparationError, SqlServerPreparationOptions,
    SqlServerSharedInstancePlan, SqlServerSharedInstancePlanOptions,
    plan_sql_server_project_resources,
};
use crate::control_plane::shared_infrastructure::{
    CredentialEntropy, CredentialSecret, SharedInstancePlan, generate_credential_secret,
};
use crate::control_plane::state::{CredentialLifecycle, StateStore};

const SQLCMD_PATH: &str = "/opt/mssql-tools18/bin/sqlcmd";

/// Reserves SQL Server-compliant bootstrap and project credentials.
pub(crate) fn prepare_sql_server_shared_instances<Store, Entropy>(
    store: &mut Store,
    shared_instances: &[SharedInstancePlan],
    entropy: &Entropy,
    options: SqlServerPreparationOptions<'_>,
) -> Result<Vec<PreparedSqlServerSharedInstance>, SqlServerPreparationError>
where
    Store: StateStore,
    Entropy: CredentialEntropy,
{
    let mut prepared = Vec::with_capacity(shared_instances.len());

    for shared in shared_instances {
        if shared.profile().implementation() != "sqlserver" {
            return Err(invalid(format!(
                "SQL Server preparation cannot materialize implementation '{}'",
                shared.profile().implementation()
            )));
        }
        let candidate = instance_plan(shared, &options, strong_secret(entropy)?)?;
        let bootstrap = store
            .insert_credential_if_absent(candidate.bootstrap_credential())
            .map_err(invalid)?;
        require_active(&bootstrap)?;
        let instance = instance_plan(
            shared,
            &options,
            CredentialSecret::new(bootstrap.secret().to_owned()),
        )?;
        let mut projects = Vec::with_capacity(shared.consumers().len());

        for consumer in shared.consumers() {
            let candidate = plan_sql_server_project_resources(
                consumer.project_id(),
                consumer.service_id(),
                &instance,
                strong_secret(entropy)?,
            )
            .map_err(invalid)?;
            let credential = store
                .insert_credential_if_absent(candidate.credential())
                .map_err(invalid)?;
            require_active(&credential)?;
            projects.push(
                plan_sql_server_project_resources(
                    consumer.project_id(),
                    consumer.service_id(),
                    &instance,
                    CredentialSecret::new(credential.secret().to_owned()),
                )
                .map_err(invalid)?,
            );
        }
        prepared.push(PreparedSqlServerSharedInstance::new(instance, projects));
    }

    Ok(prepared)
}

fn instance_plan(
    shared: &SharedInstancePlan,
    options: &SqlServerPreparationOptions<'_>,
    bootstrap_secret: CredentialSecret,
) -> Result<SqlServerSharedInstancePlan, SqlServerPreparationError> {
    SqlServerSharedInstancePlan::new(
        shared,
        SqlServerSharedInstancePlanOptions {
            installation_id: options.installation_id.to_owned(),
            network_name: options.network_name.to_owned(),
            schema_version: options.schema_version,
            desired_revision: shared.fingerprint().as_str().to_owned(),
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

fn require_active(
    credential: &crate::control_plane::state::CredentialRecord,
) -> Result<(), SqlServerPreparationError> {
    if credential.lifecycle() != CredentialLifecycle::Active {
        return Err(invalid(format!(
            "credential '{}' is disabled and requires explicit project adoption",
            credential.credential_id()
        )));
    }

    Ok(())
}

fn invalid(error: impl std::fmt::Display) -> SqlServerPreparationError {
    SqlServerPreparationError::new(error.to_string())
}
