use super::{
    MongoDbLogicalResourcePlan, MongoDbPlanError, MongoDbProjectResources,
    MongoDbSharedInstancePlan,
};
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EnvironmentLifecycle,
    ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Composes one database user, stable credential, and application environment.
pub(crate) fn plan_mongodb_project_resources(
    project_id: &str,
    service_id: &str,
    instance: &MongoDbSharedInstancePlan,
    secret: CredentialSecret,
) -> Result<MongoDbProjectResources, MongoDbPlanError> {
    let bootstrap_secret =
        CredentialSecret::new(instance.bootstrap_credential().secret().to_owned());
    let logical =
        MongoDbLogicalResourcePlan::new(project_id, service_id, secret.clone(), bootstrap_secret)?;
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: logical.credential_id().to_owned(),
        project_id: Some(project_id.to_owned()),
        service_id: service_id.to_owned(),
        username: logical.username().to_owned(),
        secret: secret.expose().to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let values = BTreeMap::from([
        (
            "MONGODB_HOST".to_owned(),
            instance.container().name().to_owned(),
        ),
        ("MONGODB_PORT".to_owned(), "27017".to_owned()),
        (
            "MONGODB_DATABASE".to_owned(),
            logical.database_name().to_owned(),
        ),
        ("MONGODB_USERNAME".to_owned(), logical.username().to_owned()),
        ("MONGODB_PASSWORD".to_owned(), secret.expose().to_owned()),
    ]);
    let canonical = serde_json::to_vec(&values).map_err(|error| {
        MongoDbPlanError::new(format!(
            "failed to encode MongoDB managed environment: {error}"
        ))
    })?;
    let revision = format!("sha256:{}", hex::encode(Sha256::digest(canonical)));
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: project_id.to_owned(),
        revision,
        values,
        lifecycle: EnvironmentLifecycle::Active,
    });

    Ok(MongoDbProjectResources::new(
        logical,
        credential,
        environment,
    ))
}
