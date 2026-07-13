use super::{
    MySqlPreparationError, MySqlPreparationOptions, MySqlSharedInstancePlan,
    MySqlSharedInstancePlanOptions, PreparedMySqlSharedInstance, plan_mysql_project_resources,
};
use crate::control_plane::shared_infrastructure::{
    CredentialEntropy, CredentialSecret, SharedInstancePlan, generate_credential_secret,
};
use crate::control_plane::state::{CredentialLifecycle, StateStore};

/// Reserves stable secrets before producing executable MySQL-family plans.
pub(crate) fn prepare_mysql_shared_instances<Store, Entropy>(
    store: &mut Store,
    shared_instances: &[SharedInstancePlan],
    entropy: &Entropy,
    options: MySqlPreparationOptions<'_>,
) -> Result<Vec<PreparedMySqlSharedInstance>, MySqlPreparationError>
where
    Store: StateStore,
    Entropy: CredentialEntropy,
{
    let mut prepared = Vec::with_capacity(shared_instances.len());

    for shared in shared_instances {
        if !matches!(shared.profile().implementation(), "mysql" | "mariadb") {
            return Err(invalid(format!(
                "MySQL-family preparation cannot materialize implementation '{}'",
                shared.profile().implementation()
            )));
        }
        let candidate = instance_plan(
            shared,
            &options,
            generate_credential_secret(entropy).map_err(invalid)?,
        )?;
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
            let candidate = plan_mysql_project_resources(
                consumer.project_id(),
                consumer.service_id(),
                &instance,
                generate_credential_secret(entropy).map_err(invalid)?,
            )
            .map_err(invalid)?;
            let credential = store
                .insert_credential_if_absent(candidate.credential())
                .map_err(invalid)?;
            require_active(&credential)?;
            projects.push(
                plan_mysql_project_resources(
                    consumer.project_id(),
                    consumer.service_id(),
                    &instance,
                    CredentialSecret::new(credential.secret().to_owned()),
                )
                .map_err(invalid)?,
            );
        }

        prepared.push(PreparedMySqlSharedInstance::new(instance, projects));
    }

    Ok(prepared)
}

fn instance_plan(
    shared: &SharedInstancePlan,
    options: &MySqlPreparationOptions<'_>,
    bootstrap_secret: CredentialSecret,
) -> Result<MySqlSharedInstancePlan, MySqlPreparationError> {
    MySqlSharedInstancePlan::new(
        shared,
        MySqlSharedInstancePlanOptions {
            installation_id: options.installation_id.to_owned(),
            network_name: options.network_name.to_owned(),
            schema_version: options.schema_version,
            desired_revision: shared.fingerprint().as_str().to_owned(),
            bootstrap_secret,
        },
    )
    .map_err(invalid)
}

fn require_active(
    credential: &crate::control_plane::state::CredentialRecord,
) -> Result<(), MySqlPreparationError> {
    if credential.lifecycle() != CredentialLifecycle::Active {
        return Err(invalid(format!(
            "credential '{}' is disabled and requires explicit project adoption",
            credential.credential_id()
        )));
    }

    Ok(())
}

fn invalid(error: impl std::fmt::Display) -> MySqlPreparationError {
    MySqlPreparationError::new(error.to_string())
}
