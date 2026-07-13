use super::{
    MySqlLogicalResourcePlan, MySqlPlanError, MySqlProjectResources, MySqlSharedInstancePlan,
};
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EnvironmentLifecycle,
    ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Composes MySQL-family SQL, credential state, and application environment.
pub(crate) fn plan_mysql_project_resources(
    project_id: &str,
    service_id: &str,
    instance: &MySqlSharedInstancePlan,
    secret: CredentialSecret,
) -> Result<MySqlProjectResources, MySqlPlanError> {
    let logical =
        MySqlLogicalResourcePlan::new(instance.flavor(), project_id, service_id, secret.clone())?;
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: logical.credential_id().to_owned(),
        project_id: Some(project_id.to_owned()),
        service_id: service_id.to_owned(),
        username: logical.username().to_owned(),
        secret: secret.expose().to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let values = BTreeMap::from([
        ("DB_CONNECTION".to_owned(), "mysql".to_owned()),
        ("DB_HOST".to_owned(), instance.container().name().to_owned()),
        ("DB_PORT".to_owned(), "3306".to_owned()),
        ("DB_DATABASE".to_owned(), logical.schema_name().to_owned()),
        ("DB_USERNAME".to_owned(), logical.username().to_owned()),
        ("DB_PASSWORD".to_owned(), secret.expose().to_owned()),
    ]);
    let canonical = serde_json::to_vec(&values).map_err(|error| {
        MySqlPlanError::new(format!(
            "failed to encode MySQL managed environment: {error}"
        ))
    })?;
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: project_id.to_owned(),
        revision: format!("sha256:{}", hex::encode(Sha256::digest(canonical))),
        values,
        lifecycle: EnvironmentLifecycle::Active,
    });

    Ok(MySqlProjectResources::new(logical, credential, environment))
}
