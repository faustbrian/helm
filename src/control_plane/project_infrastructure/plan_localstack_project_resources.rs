use super::{
    PreparedProjectService, ProjectServicePreparationError, ProjectServiceProvisioningJob,
    project_service_provisioning_images::MINIO_CLIENT_IMAGE,
};
use crate::control_plane::ServiceExecutionPlan;
use crate::control_plane::state::{
    EnvironmentLifecycle, ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Composes one persistent LocalStack gateway with private SDK defaults.
pub(crate) fn plan_localstack_project_resources(
    service: &ServiceExecutionPlan,
) -> Result<PreparedProjectService, ProjectServicePreparationError> {
    if service.desired().preset() != Some("localstack") {
        return Err(invalid(format!(
            "LocalStack preparation cannot materialize preset '{}'",
            service.desired().preset().unwrap_or("<none>")
        )));
    }

    let project_id = service.project().as_str();
    let service_id = service.service().as_str();
    let container_name = format!("stackctl-{project_id}-{service_id}");
    let bucket = container_name.clone();
    if bucket.len() > 63 {
        return Err(invalid(format!(
            "LocalStack bucket '{bucket}' exceeds the 63-byte S3 limit"
        )));
    }
    let container_environment = BTreeMap::from([
        ("GATEWAY_LISTEN".to_owned(), "0.0.0.0:4566".to_owned()),
        (
            "LOCALSTACK_HOST".to_owned(),
            format!("{container_name}:4566"),
        ),
        ("PERSISTENCE".to_owned(), "1".to_owned()),
    ]);
    for (key, value) in &container_environment {
        if service
            .desired()
            .environment()
            .get(key)
            .is_some_and(|declared| declared != value)
        {
            return Err(invalid(format!(
                "LocalStack service '{project_id}-{service_id}' cannot replace managed \
                 environment key '{key}'"
            )));
        }
    }

    let values = BTreeMap::from([
        ("AWS_ACCESS_KEY_ID".to_owned(), "test".to_owned()),
        ("AWS_BUCKET".to_owned(), bucket.clone()),
        ("AWS_DEFAULT_REGION".to_owned(), "us-east-1".to_owned()),
        (
            "AWS_ENDPOINT_URL".to_owned(),
            format!("http://{container_name}:4566"),
        ),
        ("AWS_SECRET_ACCESS_KEY".to_owned(), "test".to_owned()),
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
            format!("localstack/{bucket}"),
        ],
        BTreeMap::from([(
            "MC_HOST_localstack".to_owned(),
            format!("http://test:test@{container_name}:4566"),
        )]),
    )?;

    Ok(PreparedProjectService::new(
        project_id.to_owned(),
        service_id.to_owned(),
        None,
        environment,
        container_environment,
        None,
    )
    .with_provisioning_job(provisioning_job))
}

fn invalid(error: impl std::fmt::Display) -> ProjectServicePreparationError {
    ProjectServicePreparationError::new(error.to_string())
}
