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

/// Composes one stable Meilisearch master key and private endpoint.
pub(crate) fn plan_meilisearch_project_resources(
    service: &ServiceExecutionPlan,
    secret: CredentialSecret,
) -> Result<PreparedProjectService, ProjectServicePreparationError> {
    if service.desired().preset() != Some("meilisearch") {
        return Err(invalid(format!(
            "Meilisearch preparation cannot materialize preset '{}'",
            service.desired().preset().unwrap_or("<none>")
        )));
    }

    let project_id = service.project().as_str();
    let service_id = service.service().as_str();
    let container_name = format!("stackctl-{project_id}-{service_id}");
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("{project_id}/{service_id}/meilisearch"),
        project_id: Some(project_id.to_owned()),
        service_id: service_id.to_owned(),
        username: "master".to_owned(),
        secret: secret.expose().to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let container_environment = BTreeMap::from([
        ("MEILI_DB_PATH".to_owned(), "/meili_data".to_owned()),
        ("MEILI_HTTP_ADDR".to_owned(), "0.0.0.0:7700".to_owned()),
        ("MEILI_MASTER_KEY".to_owned(), secret.expose().to_owned()),
    ]);
    for (key, value) in &container_environment {
        if service
            .desired()
            .environment()
            .get(key)
            .is_some_and(|declared| declared != value)
        {
            return Err(invalid(format!(
                "Meilisearch service '{project_id}-{service_id}' cannot replace generated \
                 environment key '{key}'"
            )));
        }
    }
    let values = BTreeMap::from([
        (
            "MEILISEARCH_HOST".to_owned(),
            format!("http://{container_name}:7700"),
        ),
        ("MEILISEARCH_KEY".to_owned(), secret.expose().to_owned()),
    ]);
    let canonical = serde_json::to_vec(&values).map_err(invalid)?;
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: project_id.to_owned(),
        revision: format!("sha256:{}", hex::encode(Sha256::digest(canonical))),
        values,
        lifecycle: EnvironmentLifecycle::Active,
    });
    let readiness = project_service_http_readiness_job(ProjectServiceHttpReadinessOptions {
        url: format!("http://{container_name}:7700/keys"),
        authentication: ProjectServiceHttpAuthentication::Bearer {
            environment_key: "MEILI_MASTER_KEY",
            secret: secret.expose(),
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
