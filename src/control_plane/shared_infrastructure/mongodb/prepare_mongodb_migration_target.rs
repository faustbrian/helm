use super::{
    MongoDbMigrationInstancePlanOptions, MongoDbMigrationPreparationOptions,
    MongoDbPreparationError, MongoDbSharedInstancePlan,
};
use crate::control_plane::shared_infrastructure::{
    CredentialEntropy, CredentialSecret, SharedInstancePlan, generate_credential_secret,
};
use crate::control_plane::state::{CredentialLifecycle, StateStore};

/// Reserves one stable administrator before any MongoDB target mutation.
pub(crate) fn prepare_mongodb_migration_target<Store, Entropy>(
    store: &mut Store,
    shared: &SharedInstancePlan,
    entropy: &Entropy,
    options: MongoDbMigrationPreparationOptions<'_>,
) -> Result<MongoDbSharedInstancePlan, MongoDbPreparationError>
where
    Store: StateStore,
    Entropy: CredentialEntropy,
{
    let candidate = instance_plan(
        shared,
        options,
        generate_credential_secret(entropy).map_err(invalid)?,
    )?;
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
    options: MongoDbMigrationPreparationOptions<'_>,
    bootstrap_secret: CredentialSecret,
) -> Result<MongoDbSharedInstancePlan, MongoDbPreparationError> {
    let bootstrap_secret_file = options
        .state_directory
        .join("migrations")
        .join(options.migration_id)
        .join("mongodb-secrets/root-password");
    MongoDbSharedInstancePlan::new_migration_target(
        shared,
        MongoDbMigrationInstancePlanOptions {
            migration_id: options.migration_id.to_owned(),
            project_id: options.project_id.to_owned(),
            installation_id: options.installation_id.to_owned(),
            network_name: options.network_name.to_owned(),
            schema_version: options.schema_version,
            desired_revision: options.desired_revision.to_owned(),
            bootstrap_secret,
            bootstrap_secret_file,
        },
    )
    .map_err(invalid)
}

fn invalid(error: impl std::fmt::Display) -> MongoDbPreparationError {
    MongoDbPreparationError::new(error.to_string())
}
