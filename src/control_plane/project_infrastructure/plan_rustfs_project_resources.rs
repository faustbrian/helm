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

const MINIO_CLIENT_IMAGE: &str = concat!(
    "minio/mc@sha256:",
    "a7fe349ef4bd8521fb8497f55c6042871b2ae640607cf99d9bede5e9bdf11727"
);

/// Composes one authenticated single-node RustFS endpoint and data volume.
pub(crate) fn plan_rustfs_project_resources(
    service: &ServiceExecutionPlan,
    secret: CredentialSecret,
) -> Result<PreparedProjectService, ProjectServicePreparationError> {
    if service.desired().preset() != Some("rustfs") {
        return Err(invalid(format!(
            "RustFS preparation cannot materialize preset '{}'",
            service.desired().preset().unwrap_or("<none>")
        )));
    }

    let project_id = service.project().as_str();
    let service_id = service.service().as_str();
    let container_name = format!("stackctl-{project_id}-{service_id}");
    let bucket = container_name.clone();
    if bucket.len() > 63 {
        return Err(invalid(format!(
            "RustFS bucket '{bucket}' exceeds the 63-byte S3 limit"
        )));
    }
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("{project_id}/{service_id}/rustfs"),
        project_id: Some(project_id.to_owned()),
        service_id: service_id.to_owned(),
        username: "stackctl_admin".to_owned(),
        secret: secret.expose().to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let container_environment = BTreeMap::from([
        ("RUSTFS_ACCESS_KEY".to_owned(), "stackctl_admin".to_owned()),
        ("RUSTFS_ADDRESS".to_owned(), ":9000".to_owned()),
        ("RUSTFS_CONSOLE_ENABLE".to_owned(), "false".to_owned()),
        ("RUSTFS_SECRET_KEY".to_owned(), secret.expose().to_owned()),
        ("RUSTFS_VOLUMES".to_owned(), "/data".to_owned()),
    ]);
    for (key, value) in &container_environment {
        if service
            .desired()
            .environment()
            .get(key)
            .is_some_and(|declared| declared != value)
        {
            return Err(invalid(format!(
                "RustFS service '{project_id}-{service_id}' cannot replace generated \
                 environment key '{key}'"
            )));
        }
    }

    let values = BTreeMap::from([
        ("AWS_ACCESS_KEY_ID".to_owned(), "stackctl_admin".to_owned()),
        ("AWS_BUCKET".to_owned(), bucket.clone()),
        ("AWS_DEFAULT_REGION".to_owned(), "us-east-1".to_owned()),
        (
            "AWS_ENDPOINT".to_owned(),
            format!("http://{container_name}:9000"),
        ),
        (
            "AWS_SECRET_ACCESS_KEY".to_owned(),
            secret.expose().to_owned(),
        ),
        ("AWS_USE_PATH_STYLE_ENDPOINT".to_owned(), "true".to_owned()),
    ]);
    let canonical = serde_json::to_vec(&values).map_err(invalid)?;
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: project_id.to_owned(),
        revision: format!("sha256:{}", hex::encode(Sha256::digest(canonical))),
        values,
        lifecycle: EnvironmentLifecycle::Active,
    });
    let provisioning_job = ProjectServiceProvisioningJob::new(
        MINIO_CLIENT_IMAGE,
        vec![
            "mb".to_owned(),
            "--ignore-existing".to_owned(),
            format!("stackctl/{bucket}"),
        ],
        BTreeMap::from([(
            "MC_HOST_stackctl".to_owned(),
            format!(
                "http://stackctl_admin:{}@{container_name}:9000",
                secret.expose()
            ),
        )]),
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
