use super::{
    ObjectStorePlanError, ObjectStoreProjectDefinition, ObjectStoreProjectResources,
    ObjectStoreSharedInstancePlan,
};
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EnvironmentLifecycle,
    ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Composes one bucket policy, stable credential, and application environment.
pub(crate) fn plan_object_store_project_resources(
    project_id: &str,
    service_id: &str,
    instance: &ObjectStoreSharedInstancePlan,
    secret: CredentialSecret,
) -> Result<ObjectStoreProjectResources, ObjectStorePlanError> {
    let definition = ObjectStoreProjectDefinition::new(project_id, service_id, secret.clone())?;
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("{project_id}/{service_id}/object-store"),
        project_id: Some(project_id.to_owned()),
        service_id: service_id.to_owned(),
        username: definition.username().to_owned(),
        secret: secret.expose().to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let values = BTreeMap::from([
        (
            "AWS_ACCESS_KEY_ID".to_owned(),
            definition.username().to_owned(),
        ),
        ("AWS_BUCKET".to_owned(), definition.bucket().to_owned()),
        ("AWS_DEFAULT_REGION".to_owned(), "us-east-1".to_owned()),
        (
            "AWS_ENDPOINT".to_owned(),
            format!("http://{}:9000", instance.container().name()),
        ),
        (
            "AWS_SECRET_ACCESS_KEY".to_owned(),
            secret.expose().to_owned(),
        ),
        ("AWS_USE_PATH_STYLE_ENDPOINT".to_owned(), "true".to_owned()),
    ]);
    let canonical = serde_json::to_vec(&values).map_err(|error| {
        ObjectStorePlanError::new(format!(
            "failed to encode object-store managed environment: {error}"
        ))
    })?;
    let revision = format!("sha256:{}", hex::encode(Sha256::digest(canonical)));
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: project_id.to_owned(),
        revision,
        values,
        lifecycle: EnvironmentLifecycle::Active,
    });

    Ok(ObjectStoreProjectResources::new(
        definition,
        credential,
        environment,
    ))
}
