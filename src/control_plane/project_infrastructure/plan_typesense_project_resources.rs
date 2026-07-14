use super::{PreparedProjectService, ProjectServicePreparationError};
use crate::control_plane::ServiceExecutionPlan;
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EnvironmentLifecycle,
    ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Composes one stable Typesense bootstrap identity and private endpoint.
pub(crate) fn plan_typesense_project_resources(
    service: &ServiceExecutionPlan,
    secret: CredentialSecret,
) -> Result<PreparedProjectService, ProjectServicePreparationError> {
    if service.desired().preset() != Some("typesense") {
        return Err(invalid(format!(
            "Typesense preparation cannot materialize preset '{}'",
            service.desired().preset().unwrap_or("<none>")
        )));
    }

    let project_id = service.project().as_str();
    let service_id = service.service().as_str();
    let container_name = format!("stackctl-{project_id}-{service_id}");
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("{project_id}/{service_id}/typesense"),
        project_id: Some(project_id.to_owned()),
        service_id: service_id.to_owned(),
        username: "bootstrap".to_owned(),
        secret: secret.expose().to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let container_environment = BTreeMap::from([
        ("TYPESENSE_API_KEY".to_owned(), secret.expose().to_owned()),
        ("TYPESENSE_DATA_DIR".to_owned(), "/data".to_owned()),
    ]);
    for (key, value) in &container_environment {
        if service
            .desired()
            .environment()
            .get(key)
            .is_some_and(|declared| declared != value)
        {
            return Err(invalid(format!(
                "Typesense service '{project_id}-{service_id}' cannot replace generated \
                 environment key '{key}'"
            )));
        }
    }
    let values = BTreeMap::from([
        ("TYPESENSE_API_KEY".to_owned(), secret.expose().to_owned()),
        ("TYPESENSE_HOST".to_owned(), container_name),
        ("TYPESENSE_PORT".to_owned(), "8108".to_owned()),
        ("TYPESENSE_PROTOCOL".to_owned(), "http".to_owned()),
    ]);
    let canonical = serde_json::to_vec(&values).map_err(invalid)?;
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: project_id.to_owned(),
        revision: format!("sha256:{}", hex::encode(Sha256::digest(canonical))),
        values,
        lifecycle: EnvironmentLifecycle::Active,
    });

    Ok(PreparedProjectService::new(
        project_id.to_owned(),
        service_id.to_owned(),
        Some(credential),
        environment,
        container_environment,
        None,
    ))
}

fn invalid(error: impl std::fmt::Display) -> ProjectServicePreparationError {
    ProjectServicePreparationError::new(error.to_string())
}
