use super::{PostgresLogicalResourcePlan, PostgresPlanError, PostgresProjectResources};
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EnvironmentLifecycle,
    ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Composes PostgreSQL project resources for an already-owned container host.
pub(crate) fn plan_postgres_project_resources_for_host(
    project_id: &str,
    service_id: &str,
    container_host: &str,
    secret: CredentialSecret,
) -> Result<PostgresProjectResources, PostgresPlanError> {
    if container_host.is_empty() {
        return Err(PostgresPlanError::new(
            "PostgreSQL container host must not be empty".to_owned(),
        ));
    }
    let logical = PostgresLogicalResourcePlan::new(project_id, service_id, secret.clone())?;
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: logical.credential_id().to_owned(),
        project_id: Some(project_id.to_owned()),
        service_id: service_id.to_owned(),
        username: logical.role_name().to_owned(),
        secret: secret.expose().to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let values = BTreeMap::from([
        ("DB_CONNECTION".to_owned(), "pgsql".to_owned()),
        ("DB_HOST".to_owned(), container_host.to_owned()),
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

#[cfg(test)]
mod tests {
    use super::plan_postgres_project_resources_for_host;
    use crate::control_plane::shared_infrastructure::CredentialSecret;

    #[test]
    fn retained_source_environment_uses_the_observed_source_host() {
        let resources = plan_postgres_project_resources_for_host(
            "api",
            "database",
            "stackctl-shared-postgres-17",
            CredentialSecret::new("secret".to_owned()),
        )
        .expect("retained source resources");

        assert_eq!(
            resources.environment().values().get("DB_HOST"),
            Some(&"stackctl-shared-postgres-17".to_owned())
        );
        assert_eq!(
            resources.environment().values().get("DB_DATABASE"),
            Some(&"stackctl_api_database".to_owned())
        );
    }
}
