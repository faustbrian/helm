use super::{
    PostgresPreparationError, PostgresPreparationOptions, PostgresSharedInstancePlan,
    PostgresSharedInstancePlanOptions, PreparedPostgresSharedInstance,
    plan_postgres_project_resources,
};
use crate::control_plane::shared_infrastructure::{
    CredentialEntropy, CredentialSecret, SharedInstancePlan, generate_credential_secret,
};
use crate::control_plane::state::{CredentialLifecycle, StateStore};

/// Reserves stable secrets before producing executable PostgreSQL plans.
pub(crate) fn prepare_postgres_shared_instances<Store, Entropy>(
    store: &mut Store,
    shared_instances: &[SharedInstancePlan],
    entropy: &Entropy,
    options: PostgresPreparationOptions<'_>,
) -> Result<Vec<PreparedPostgresSharedInstance>, PostgresPreparationError>
where
    Store: StateStore,
    Entropy: CredentialEntropy,
{
    let mut prepared = Vec::with_capacity(shared_instances.len());

    for shared in shared_instances {
        if shared.profile().implementation() != "postgresql" {
            return Err(invalid(format!(
                "PostgreSQL preparation cannot materialize implementation '{}'",
                shared.profile().implementation()
            )));
        }
        let bootstrap_candidate = generate_credential_secret(entropy).map_err(invalid)?;
        let candidate_instance = instance_plan(shared, &options, bootstrap_candidate)?;
        let bootstrap = store
            .insert_credential_if_absent(candidate_instance.bootstrap_credential())
            .map_err(invalid)?;
        require_active(&bootstrap)?;
        let instance = instance_plan(
            shared,
            &options,
            CredentialSecret::new(bootstrap.secret().to_owned()),
        )?;
        let mut projects = Vec::with_capacity(shared.consumers().len());

        for consumer in shared.consumers() {
            let candidate_secret = generate_credential_secret(entropy).map_err(invalid)?;
            let candidate = plan_postgres_project_resources(
                consumer.project_id(),
                consumer.service_id(),
                &instance,
                candidate_secret,
            )
            .map_err(invalid)?;
            let credential = store
                .insert_credential_if_absent(candidate.credential())
                .map_err(invalid)?;
            require_active(&credential)?;
            projects.push(
                plan_postgres_project_resources(
                    consumer.project_id(),
                    consumer.service_id(),
                    &instance,
                    CredentialSecret::new(credential.secret().to_owned()),
                )
                .map_err(invalid)?,
            );
        }

        prepared.push(PreparedPostgresSharedInstance::new(instance, projects));
    }

    Ok(prepared)
}

fn instance_plan(
    shared: &SharedInstancePlan,
    options: &PostgresPreparationOptions<'_>,
    bootstrap_secret: CredentialSecret,
) -> Result<PostgresSharedInstancePlan, PostgresPreparationError> {
    PostgresSharedInstancePlan::new(
        shared,
        PostgresSharedInstancePlanOptions {
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
) -> Result<(), PostgresPreparationError> {
    if credential.lifecycle() != CredentialLifecycle::Active {
        return Err(invalid(format!(
            "credential '{}' is disabled and requires explicit project adoption",
            credential.credential_id()
        )));
    }

    Ok(())
}

fn invalid(error: impl std::fmt::Display) -> PostgresPreparationError {
    PostgresPreparationError::new(error.to_string())
}
