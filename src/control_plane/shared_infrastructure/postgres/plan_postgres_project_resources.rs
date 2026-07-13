use super::{
    PostgresLogicalResourcePlan, PostgresPlanError, PostgresProjectResources,
    PostgresSharedInstancePlan,
};
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EnvironmentLifecycle,
    ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Composes logical SQL, stable credential state, and application environment.
pub(crate) fn plan_postgres_project_resources(
    project_id: &str,
    service_id: &str,
    instance: &PostgresSharedInstancePlan,
    secret: CredentialSecret,
) -> Result<PostgresProjectResources, PostgresPlanError> {
    let logical = PostgresLogicalResourcePlan::new(project_id, service_id, secret.clone())?;
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: logical.credential_id().to_owned(),
        project_id: project_id.to_owned(),
        service_id: service_id.to_owned(),
        username: logical.role_name().to_owned(),
        secret: secret.expose().to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let values = BTreeMap::from([
        ("DB_CONNECTION".to_owned(), "pgsql".to_owned()),
        ("DB_HOST".to_owned(), instance.container().name().to_owned()),
        ("DB_PORT".to_owned(), "5432".to_owned()),
        ("DB_DATABASE".to_owned(), logical.database_name().to_owned()),
        ("DB_USERNAME".to_owned(), logical.role_name().to_owned()),
        ("DB_PASSWORD".to_owned(), secret.expose().to_owned()),
    ]);
    let canonical = serde_json::to_vec(&values).map_err(|error| {
        PostgresPlanError::new(format!(
            "failed to encode PostgreSQL managed environment: {error}"
        ))
    })?;
    let revision = format!("sha256:{}", hex::encode(Sha256::digest(canonical)));
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: project_id.to_owned(),
        revision,
        values,
        lifecycle: EnvironmentLifecycle::Active,
    });

    Ok(PostgresProjectResources::new(
        logical,
        credential,
        environment,
    ))
}
