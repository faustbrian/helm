use super::{PreparedProjectService, ProjectServicePreparationError};
use crate::control_plane::ServiceExecutionPlan;
use crate::control_plane::gateway::GatewayRoute;
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EnvironmentLifecycle,
    ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Composes the complete Pusher-compatible identity and route for one Soketi service.
pub(crate) fn plan_soketi_project_resources(
    service: &ServiceExecutionPlan,
    secret: CredentialSecret,
) -> Result<PreparedProjectService, ProjectServicePreparationError> {
    if service.desired().preset() != Some("soketi") {
        return Err(invalid(format!(
            "Soketi preparation cannot materialize preset '{}'",
            service.desired().preset().unwrap_or("<none>")
        )));
    }

    let project_id = service.project().as_str();
    let service_id = service.service().as_str();
    let application_id = format!("{project_id}-{service_id}");
    let application_key = stable_application_key(project_id, service_id);
    let container_name = format!("stackctl-{project_id}-{service_id}");
    let domain = format!("{project_id}-{service_id}.stackctl.localhost");
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("{project_id}/{service_id}/soketi"),
        project_id: Some(project_id.to_owned()),
        service_id: service_id.to_owned(),
        username: application_key.clone(),
        secret: secret.expose().to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let container_environment = BTreeMap::from([
        ("SOKETI_DEFAULT_APP_ID".to_owned(), application_id.clone()),
        ("SOKETI_DEFAULT_APP_KEY".to_owned(), application_key.clone()),
        (
            "SOKETI_DEFAULT_APP_SECRET".to_owned(),
            secret.expose().to_owned(),
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
                "Soketi service '{project_id}-{service_id}' cannot replace generated \
                 environment key '{key}'"
            )));
        }
    }
    let values = BTreeMap::from([
        ("PUSHER_APP_ID".to_owned(), application_id),
        ("PUSHER_APP_KEY".to_owned(), application_key.clone()),
        ("PUSHER_APP_SECRET".to_owned(), secret.expose().to_owned()),
        ("PUSHER_HOST".to_owned(), container_name.clone()),
        ("PUSHER_PORT".to_owned(), "6001".to_owned()),
        ("PUSHER_SCHEME".to_owned(), "http".to_owned()),
        ("VITE_PUSHER_APP_KEY".to_owned(), application_key),
        ("VITE_PUSHER_HOST".to_owned(), domain.clone()),
        ("VITE_PUSHER_PORT".to_owned(), "443".to_owned()),
        ("VITE_PUSHER_SCHEME".to_owned(), "https".to_owned()),
    ]);
    let canonical = serde_json::to_vec(&values).map_err(invalid)?;
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: project_id.to_owned(),
        revision: format!("sha256:{}", hex::encode(Sha256::digest(canonical))),
        values,
        lifecycle: EnvironmentLifecycle::Active,
    });
    let route =
        GatewayRoute::new(domain, format!("http://{container_name}:6001")).map_err(invalid)?;

    Ok(PreparedProjectService::new(
        project_id.to_owned(),
        service_id.to_owned(),
        credential,
        environment,
        container_environment,
        route,
    ))
}

fn stable_application_key(project_id: &str, service_id: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"stackctl-soketi-application-key-v1\0");
    digest.update(project_id.as_bytes());
    digest.update([0]);
    digest.update(service_id.as_bytes());

    hex::encode(digest.finalize())[..32].to_owned()
}

fn invalid(error: impl std::fmt::Display) -> ProjectServicePreparationError {
    ProjectServicePreparationError::new(error.to_string())
}
