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

/// Composes one stable Elasticsearch administrator and private HTTP endpoint.
pub(crate) fn plan_elasticsearch_project_resources(
    service: &ServiceExecutionPlan,
    password: CredentialSecret,
) -> Result<PreparedProjectService, ProjectServicePreparationError> {
    if service.desired().preset() != Some("elasticsearch") {
        return Err(invalid(format!(
            "Elasticsearch preparation cannot materialize preset '{}'",
            service.desired().preset().unwrap_or("<none>")
        )));
    }

    let project_id = service.project().as_str();
    let service_id = service.service().as_str();
    let container_name = format!("stackctl-{project_id}-{service_id}");
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("{project_id}/{service_id}/elasticsearch"),
        project_id: Some(project_id.to_owned()),
        service_id: service_id.to_owned(),
        username: "elastic".to_owned(),
        secret: password.expose().to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let container_environment = BTreeMap::from([
        ("ELASTIC_PASSWORD".to_owned(), password.expose().to_owned()),
        ("discovery.type".to_owned(), "single-node".to_owned()),
        (
            "xpack.security.autoconfiguration.enabled".to_owned(),
            "false".to_owned(),
        ),
        ("xpack.security.enabled".to_owned(), "true".to_owned()),
        (
            "xpack.security.http.ssl.enabled".to_owned(),
            "false".to_owned(),
        ),
    ]);
    for (key, value) in &container_environment {
        if service
            .desired()
            .environment()
            .get(key)
            .is_some_and(|declared| declared != value)
        {
            return Err(invalid(format!(
                "Elasticsearch service '{project_id}-{service_id}' cannot replace generated \
                 environment key '{key}'"
            )));
        }
    }
    let values = BTreeMap::from([
        (
            "ELASTICSEARCH_PASSWORD".to_owned(),
            password.expose().to_owned(),
        ),
        (
            "ELASTICSEARCH_URL".to_owned(),
            format!("http://{container_name}:9200"),
        ),
        ("ELASTICSEARCH_USERNAME".to_owned(), "elastic".to_owned()),
    ]);
    let canonical = serde_json::to_vec(&values).map_err(invalid)?;
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: project_id.to_owned(),
        revision: format!("sha256:{}", hex::encode(Sha256::digest(canonical))),
        values,
        lifecycle: EnvironmentLifecycle::Active,
    });
    let readiness = project_service_http_readiness_job(ProjectServiceHttpReadinessOptions {
        url: format!("http://{container_name}:9200/_cluster/health"),
        authentication: ProjectServiceHttpAuthentication::Basic {
            username: "elastic",
            environment_key: "ELASTIC_PASSWORD",
            secret: password.expose(),
        },
        allow_invalid_certificate: false,
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
