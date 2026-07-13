use super::{
    MongoDbPreparationError, MongoDbPreparationOptions, MongoDbSharedInstancePlan,
    MongoDbSharedInstancePlanOptions, PreparedMongoDbSharedInstance,
    plan_mongodb_project_resources,
};
use crate::control_plane::shared_infrastructure::{
    CredentialEntropy, CredentialSecret, SharedInstancePlan, generate_credential_secret,
};
use crate::control_plane::state::{CredentialLifecycle, StateStore};

/// Reserves stable bootstrap and tenant secrets before Engine mutation.
pub(crate) fn prepare_mongodb_shared_instances<Store, Entropy>(
    store: &mut Store,
    shared_instances: &[SharedInstancePlan],
    entropy: &Entropy,
    options: MongoDbPreparationOptions<'_>,
) -> Result<Vec<PreparedMongoDbSharedInstance>, MongoDbPreparationError>
where
    Store: StateStore,
    Entropy: CredentialEntropy,
{
    let mut prepared = Vec::with_capacity(shared_instances.len());

    for shared in shared_instances {
        if shared.profile().implementation() != "mongodb" {
            return Err(invalid(format!(
                "MongoDB preparation cannot materialize implementation '{}'",
                shared.profile().implementation()
            )));
        }
        let identity = shared
            .fingerprint()
            .as_str()
            .strip_prefix("sha256:")
            .ok_or_else(|| invalid("MongoDB fingerprint is malformed"))?;
        let bootstrap_secret_file = options
            .state_directory
            .join("shared")
            .join(identity)
            .join("mongodb-secrets/root-password");
        let candidate = instance_plan(
            shared,
            &options,
            &bootstrap_secret_file,
            generate_credential_secret(entropy).map_err(invalid)?,
        )?;
        let bootstrap = store
            .insert_credential_if_absent(candidate.bootstrap_credential())
            .map_err(invalid)?;
        require_active(&bootstrap)?;
        let instance = instance_plan(
            shared,
            &options,
            &bootstrap_secret_file,
            CredentialSecret::new(bootstrap.secret().to_owned()),
        )?;
        let mut projects = Vec::with_capacity(shared.consumers().len());

        for consumer in shared.consumers() {
            let candidate = plan_mongodb_project_resources(
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
                plan_mongodb_project_resources(
                    consumer.project_id(),
                    consumer.service_id(),
                    &instance,
                    CredentialSecret::new(credential.secret().to_owned()),
                )
                .map_err(invalid)?,
            );
        }
        prepared.push(PreparedMongoDbSharedInstance::new(
            instance,
            projects,
            bootstrap_secret_file,
        ));
    }

    Ok(prepared)
}

fn instance_plan(
    shared: &SharedInstancePlan,
    options: &MongoDbPreparationOptions<'_>,
    bootstrap_secret_file: &std::path::Path,
    bootstrap_secret: CredentialSecret,
) -> Result<MongoDbSharedInstancePlan, MongoDbPreparationError> {
    MongoDbSharedInstancePlan::new(
        shared,
        MongoDbSharedInstancePlanOptions {
            installation_id: options.installation_id.to_owned(),
            network_name: options.network_name.to_owned(),
            schema_version: options.schema_version,
            desired_revision: shared.fingerprint().as_str().to_owned(),
            bootstrap_secret,
            bootstrap_secret_file: bootstrap_secret_file.to_path_buf(),
        },
    )
    .map_err(invalid)
}

fn require_active(
    credential: &crate::control_plane::state::CredentialRecord,
) -> Result<(), MongoDbPreparationError> {
    if credential.lifecycle() != CredentialLifecycle::Active {
        return Err(invalid(format!(
            "credential '{}' is disabled and requires explicit project adoption",
            credential.credential_id()
        )));
    }

    Ok(())
}

fn invalid(error: impl std::fmt::Display) -> MongoDbPreparationError {
    MongoDbPreparationError::new(error.to_string())
}
