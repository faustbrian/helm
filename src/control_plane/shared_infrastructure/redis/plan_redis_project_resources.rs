use super::{RedisAclProject, RedisPlanError, RedisProjectResources, RedisSharedInstancePlan};
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EnvironmentLifecycle,
    ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Composes one cache ACL user, stable credential, and application environment.
pub(crate) fn plan_redis_project_resources(
    project_id: &str,
    service_id: &str,
    instance: &RedisSharedInstancePlan,
    secret: CredentialSecret,
) -> Result<RedisProjectResources, RedisPlanError> {
    let acl = RedisAclProject::new(project_id, service_id, secret.clone())?;
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!(
            "{project_id}/{service_id}/{}",
            instance.flavor().implementation()
        ),
        project_id: Some(project_id.to_owned()),
        service_id: service_id.to_owned(),
        username: acl.username().to_owned(),
        secret: secret.expose().to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let values = BTreeMap::from([
        (
            "REDIS_HOST".to_owned(),
            instance.container().name().to_owned(),
        ),
        ("REDIS_PORT".to_owned(), "6379".to_owned()),
        ("REDIS_USERNAME".to_owned(), acl.username().to_owned()),
        ("REDIS_PASSWORD".to_owned(), secret.expose().to_owned()),
        ("REDIS_PREFIX".to_owned(), acl.prefix().to_owned()),
    ]);
    let canonical = serde_json::to_vec(&values).map_err(|error| {
        RedisPlanError::new(format!(
            "failed to encode Redis-compatible managed environment: {error}"
        ))
    })?;
    let revision = format!("sha256:{}", hex::encode(Sha256::digest(canonical)));
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: project_id.to_owned(),
        revision,
        values,
        lifecycle: EnvironmentLifecycle::Active,
    });

    Ok(RedisProjectResources::new(acl, credential, environment))
}
