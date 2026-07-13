use super::{RabbitMqPlanError, RabbitMqProjectDefinition, RabbitMqProjectResources};
use crate::control_plane::shared_infrastructure::{CredentialSecret, RabbitMqSharedInstancePlan};
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EnvironmentLifecycle,
    ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Composes one vhost definition, stable credential, and application environment.
pub(crate) fn plan_rabbitmq_project_resources(
    project_id: &str,
    service_id: &str,
    instance: &RabbitMqSharedInstancePlan,
    secret: CredentialSecret,
) -> Result<RabbitMqProjectResources, RabbitMqPlanError> {
    let definition = RabbitMqProjectDefinition::new(project_id, service_id, secret.clone())?;
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("{project_id}/{service_id}/rabbitmq"),
        project_id: Some(project_id.to_owned()),
        service_id: service_id.to_owned(),
        username: definition.username().to_owned(),
        secret: secret.expose().to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let values = BTreeMap::from([
        (
            "RABBITMQ_HOST".to_owned(),
            instance.container().name().to_owned(),
        ),
        ("RABBITMQ_PORT".to_owned(), "5672".to_owned()),
        (
            "RABBITMQ_USERNAME".to_owned(),
            definition.username().to_owned(),
        ),
        ("RABBITMQ_PASSWORD".to_owned(), secret.expose().to_owned()),
        ("RABBITMQ_VHOST".to_owned(), definition.vhost().to_owned()),
    ]);
    let canonical = serde_json::to_vec(&values).map_err(|error| {
        RabbitMqPlanError::new(format!(
            "failed to encode RabbitMQ managed environment: {error}"
        ))
    })?;
    let revision = format!("sha256:{}", hex::encode(Sha256::digest(canonical)));
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: project_id.to_owned(),
        revision,
        values,
        lifecycle: EnvironmentLifecycle::Active,
    });

    Ok(RabbitMqProjectResources::new(
        definition,
        credential,
        environment,
    ))
}
