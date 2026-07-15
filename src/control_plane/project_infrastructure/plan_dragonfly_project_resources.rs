use super::{
    PreparedProjectService, ProjectServicePreparationError, ProjectServiceProvisioningJob,
};
use crate::control_plane::ServiceExecutionPlan;
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EnvironmentLifecycle,
    ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const REDIS_CLIENT_IMAGE: &str = concat!(
    "redis@sha256:",
    "6ab0b6e7381779332f97b8ca76193e45b0756f38d4c0dcda72dbb3c32061ab99"
);

/// Composes one authenticated Dragonfly endpoint with scheduled snapshots.
pub(crate) fn plan_dragonfly_project_resources(
    service: &ServiceExecutionPlan,
    password: CredentialSecret,
) -> Result<PreparedProjectService, ProjectServicePreparationError> {
    if service.desired().preset() != Some("dragonfly") {
        return Err(invalid(format!(
            "Dragonfly preparation cannot materialize preset '{}'",
            service.desired().preset().unwrap_or("<none>")
        )));
    }

    let project_id = service.project().as_str();
    let service_id = service.service().as_str();
    let container_name = format!("stackctl-{project_id}-{service_id}");
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("{project_id}/{service_id}/dragonfly"),
        project_id: Some(project_id.to_owned()),
        service_id: service_id.to_owned(),
        username: "default".to_owned(),
        secret: password.expose().to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let container_environment = BTreeMap::from([
        ("DFLY_bind".to_owned(), "0.0.0.0".to_owned()),
        ("DFLY_dir".to_owned(), "/data".to_owned()),
        (
            "DFLY_primary_port_http_enabled".to_owned(),
            "false".to_owned(),
        ),
        ("DFLY_requirepass".to_owned(), password.expose().to_owned()),
        ("DFLY_snapshot_cron".to_owned(), "* * * * *".to_owned()),
    ]);
    for (key, value) in &container_environment {
        if service
            .desired()
            .environment()
            .get(key)
            .is_some_and(|declared| declared != value)
        {
            return Err(invalid(format!(
                "Dragonfly service '{project_id}-{service_id}' cannot replace generated \
                 environment key '{key}'"
            )));
        }
    }

    let values = BTreeMap::from([
        ("DRAGONFLY_HOST".to_owned(), container_name.clone()),
        (
            "DRAGONFLY_PASSWORD".to_owned(),
            password.expose().to_owned(),
        ),
        ("DRAGONFLY_PORT".to_owned(), "6379".to_owned()),
        ("DRAGONFLY_USERNAME".to_owned(), "default".to_owned()),
    ]);
    let canonical = serde_json::to_vec(&values).map_err(invalid)?;
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: project_id.to_owned(),
        revision: format!("sha256:{}", hex::encode(Sha256::digest(canonical))),
        values,
        lifecycle: EnvironmentLifecycle::Active,
    });
    let provisioning_job = ProjectServiceProvisioningJob::new(
        REDIS_CLIENT_IMAGE,
        vec![
            "redis-cli".to_owned(),
            "-e".to_owned(),
            "-h".to_owned(),
            container_name,
            "-p".to_owned(),
            "6379".to_owned(),
            "ping".to_owned(),
        ],
        BTreeMap::from([("REDISCLI_AUTH".to_owned(), password.expose().to_owned())]),
    )?;

    Ok(PreparedProjectService::new(
        project_id.to_owned(),
        service_id.to_owned(),
        Some(credential),
        environment,
        container_environment,
        None,
    )
    .with_provisioning_job(provisioning_job))
}

fn invalid(error: impl std::fmt::Display) -> ProjectServicePreparationError {
    ProjectServicePreparationError::new(error.to_string())
}
