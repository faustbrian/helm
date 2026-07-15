use super::project_service_http_readiness_job::project_service_http_readiness_job;
use super::project_service_http_readiness_options::{
    ProjectServiceHttpAuthentication, ProjectServiceHttpReadinessOptions,
};
use super::{PreparedProjectService, ProjectServicePreparationError};
use crate::control_plane::ServiceExecutionPlan;
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EnvironmentLifecycle,
    ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Adds every character class required by the OpenSearch demo installer.
pub(crate) fn opensearch_initial_admin_password(secret: CredentialSecret) -> CredentialSecret {
    CredentialSecret::new(format!("Aa1!{}", secret.expose()))
}

/// Composes one stable OpenSearch administrator and single-node endpoint.
pub(crate) fn plan_opensearch_project_resources(
    service: &ServiceExecutionPlan,
    password: CredentialSecret,
) -> Result<PreparedProjectService, ProjectServicePreparationError> {
    if service.desired().preset() != Some("opensearch") {
        return Err(invalid(format!(
            "OpenSearch preparation cannot materialize preset '{}'",
            service.desired().preset().unwrap_or("<none>")
        )));
    }

    let project_id = service.project().as_str();
    let service_id = service.service().as_str();
    let container_name = format!("stackctl-{project_id}-{service_id}");
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("{project_id}/{service_id}/opensearch"),
        project_id: Some(project_id.to_owned()),
        service_id: service_id.to_owned(),
        username: "admin".to_owned(),
        secret: password.expose().to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let container_environment = BTreeMap::from([
        (
            "OPENSEARCH_INITIAL_ADMIN_PASSWORD".to_owned(),
            password.expose().to_owned(),
        ),
        ("discovery.type".to_owned(), "single-node".to_owned()),
    ]);
    for (key, value) in &container_environment {
        if service
            .desired()
            .environment()
            .get(key)
            .is_some_and(|declared| declared != value)
        {
            return Err(invalid(format!(
                "OpenSearch service '{project_id}-{service_id}' cannot replace generated \
                 environment key '{key}'"
            )));
        }
    }
    let values = BTreeMap::from([
        (
            "OPENSEARCH_PASSWORD".to_owned(),
            password.expose().to_owned(),
        ),
        (
            "OPENSEARCH_URL".to_owned(),
            format!("https://{container_name}:9200"),
        ),
        ("OPENSEARCH_USERNAME".to_owned(), "admin".to_owned()),
    ]);
    let canonical = serde_json::to_vec(&values).map_err(invalid)?;
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: project_id.to_owned(),
        revision: format!("sha256:{}", hex::encode(Sha256::digest(canonical))),
        values,
        lifecycle: EnvironmentLifecycle::Active,
    });
    let readiness = project_service_http_readiness_job(ProjectServiceHttpReadinessOptions {
        url: format!("https://{container_name}:9200/_cluster/health"),
        authentication: ProjectServiceHttpAuthentication::Basic {
            username: "admin",
            environment_key: "OPENSEARCH_PASSWORD",
            secret: password.expose(),
        },
        allow_invalid_certificate: true,
    })?;

    Ok(PreparedProjectService::new(
        project_id.to_owned(),
        service_id.to_owned(),
        Some(credential),
        environment,
        container_environment,
        None,
    )
    .with_provisioning_job(readiness))
}

fn invalid(error: impl std::fmt::Display) -> ProjectServicePreparationError {
    ProjectServicePreparationError::new(error.to_string())
}
