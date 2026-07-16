use super::{
    PreparedRabbitMqSharedInstance, RabbitMqDefinitions, RabbitMqPreparationError,
    RabbitMqPreparationOptions, RabbitMqSharedInstancePlan, RabbitMqSharedInstancePlanOptions,
    plan_rabbitmq_project_resources,
};
use crate::control_plane::shared_infrastructure::{
    CredentialEntropy, CredentialSecret, SharedInstancePlan, generate_credential_secret,
    shared_identity_hex,
};
use crate::control_plane::state::{CredentialLifecycle, StateStore};

/// Reserves stable project credentials and composes one complete definitions set.
pub(crate) fn prepare_rabbitmq_shared_instances<Store, Entropy>(
    store: &mut Store,
    shared_instances: &[SharedInstancePlan],
    entropy: &Entropy,
    options: RabbitMqPreparationOptions<'_>,
) -> Result<Vec<PreparedRabbitMqSharedInstance>, RabbitMqPreparationError>
where
    Store: StateStore,
    Entropy: CredentialEntropy,
{
    let mut prepared = Vec::with_capacity(shared_instances.len());

    for shared in shared_instances {
        if shared.profile().implementation() != "rabbitmq" {
            return Err(invalid(format!(
                "RabbitMQ preparation cannot materialize implementation '{}'",
                shared.profile().implementation()
            )));
        }
        let identity = shared
            .fingerprint()
            .as_str()
            .strip_prefix("sha256:")
            .ok_or_else(|| invalid("RabbitMQ fingerprint is malformed"))?;
        let state_directory = options
            .state_directory
            .join("shared")
            .join(options.installation_id)
            .join(shared_identity_hex(identity))
            .join("rabbitmq-definitions");
        let instance = RabbitMqSharedInstancePlan::new(
            shared,
            RabbitMqSharedInstancePlanOptions {
                installation_id: options.installation_id.to_owned(),
                network_name: options.network_name.to_owned(),
                schema_version: options.schema_version,
                desired_revision: shared.fingerprint().as_str().to_owned(),
                definitions_directory: state_directory.join("mounted"),
            },
        )
        .map_err(invalid)?;
        let mut projects = Vec::with_capacity(shared.consumers().len());

        for consumer in shared.consumers() {
            let candidate = plan_rabbitmq_project_resources(
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
                plan_rabbitmq_project_resources(
                    consumer.project_id(),
                    consumer.service_id(),
                    &instance,
                    CredentialSecret::new(credential.secret().to_owned()),
                )
                .map_err(invalid)?,
            );
        }
        let definitions = RabbitMqDefinitions::new(
            projects
                .iter()
                .map(|project| project.definition().clone())
                .collect(),
        )
        .map_err(invalid)?;
        prepared.push(PreparedRabbitMqSharedInstance::new(
            instance,
            projects,
            definitions,
            state_directory,
        ));
    }

    Ok(prepared)
}

fn require_active(
    credential: &crate::control_plane::state::CredentialRecord,
) -> Result<(), RabbitMqPreparationError> {
    if credential.lifecycle() != CredentialLifecycle::Active {
        return Err(invalid(format!(
            "credential '{}' is disabled and requires explicit project adoption",
            credential.credential_id()
        )));
    }

    Ok(())
}

fn invalid(error: impl std::fmt::Display) -> RabbitMqPreparationError {
    RabbitMqPreparationError::new(error.to_string())
}
