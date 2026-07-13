use super::{
    SqlServerLogicalResourcePlan, SqlServerPlanError, SqlServerProjectResources,
    SqlServerSharedInstancePlan,
};
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EnvironmentLifecycle,
    ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Composes one database/login, stable credential, and application environment.
pub(crate) fn plan_sql_server_project_resources(
    project_id: &str,
    service_id: &str,
    instance: &SqlServerSharedInstancePlan,
    secret: CredentialSecret,
) -> Result<SqlServerProjectResources, SqlServerPlanError> {
    let logical = SqlServerLogicalResourcePlan::new(project_id, service_id, secret.clone())?;
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: logical.credential_id().to_owned(),
        project_id: Some(project_id.to_owned()),
        service_id: service_id.to_owned(),
        username: logical.username().to_owned(),
        secret: secret.expose().to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let values = BTreeMap::from([
        ("DB_CONNECTION".to_owned(), "sqlsrv".to_owned()),
        ("DB_DATABASE".to_owned(), logical.database_name().to_owned()),
        ("DB_HOST".to_owned(), instance.container().name().to_owned()),
        ("DB_PASSWORD".to_owned(), secret.expose().to_owned()),
        ("DB_PORT".to_owned(), "1433".to_owned()),
        ("DB_USERNAME".to_owned(), logical.username().to_owned()),
    ]);
    let canonical = serde_json::to_vec(&values).map_err(|error| {
        SqlServerPlanError::new(format!(
            "failed to encode SQL Server managed environment: {error}"
        ))
    })?;
    let revision = format!("sha256:{}", hex::encode(Sha256::digest(canonical)));
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: project_id.to_owned(),
        revision,
        values,
        lifecycle: EnvironmentLifecycle::Active,
    });

    Ok(SqlServerProjectResources::new(
        logical,
        credential,
        environment,
    ))
}
