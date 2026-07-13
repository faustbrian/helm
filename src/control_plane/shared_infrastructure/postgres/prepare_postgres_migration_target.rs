use super::{
    PostgresMigrationInstancePlanOptions, PostgresMigrationPreparationOptions,
    PostgresPreparationError, PostgresSharedInstancePlan,
};
use crate::control_plane::shared_infrastructure::{
    CredentialEntropy, CredentialSecret, SharedInstancePlan, generate_credential_secret,
};
use crate::control_plane::state::{CredentialLifecycle, StateStore};

/// Reserves one stable administrator before any migration target Engine mutation.
pub(crate) fn prepare_postgres_migration_target<Store, Entropy>(
    store: &mut Store,
    shared: &SharedInstancePlan,
    entropy: &Entropy,
    options: PostgresMigrationPreparationOptions<'_>,
) -> Result<PostgresSharedInstancePlan, PostgresPreparationError>
where
    Store: StateStore,
    Entropy: CredentialEntropy,
{
    let candidate_secret = generate_credential_secret(entropy).map_err(invalid)?;
    let candidate = instance_plan(shared, options, candidate_secret)?;
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
    options: PostgresMigrationPreparationOptions<'_>,
    bootstrap_secret: CredentialSecret,
) -> Result<PostgresSharedInstancePlan, PostgresPreparationError> {
    PostgresSharedInstancePlan::new_migration_target(
        shared,
        PostgresMigrationInstancePlanOptions {
            migration_id: options.migration_id.to_owned(),
            project_id: options.project_id.to_owned(),
            installation_id: options.installation_id.to_owned(),
            network_name: options.network_name.to_owned(),
            schema_version: options.schema_version,
            desired_revision: options.desired_revision.to_owned(),
            bootstrap_secret,
        },
    )
    .map_err(invalid)
}

fn invalid(error: impl std::fmt::Display) -> PostgresPreparationError {
    PostgresPreparationError::new(error.to_string())
}
