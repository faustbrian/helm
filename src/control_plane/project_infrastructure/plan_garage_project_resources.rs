use super::{
    PreparedProjectService, ProjectServiceContainerConfiguration, ProjectServicePreparationError,
};
use crate::control_plane::ServiceExecutionPlan;
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EnvironmentLifecycle,
    ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const GENERATED_COMMAND: [&str; 4] = ["/garage", "server", "--single-node", "--default-bucket"];

/// Composes one self-bootstrapping single-node Garage object store.
pub(crate) fn plan_garage_project_resources(
    service: &ServiceExecutionPlan,
    secret: CredentialSecret,
) -> Result<PreparedProjectService, ProjectServicePreparationError> {
    if service.desired().preset() != Some("garage") {
        return Err(invalid(format!(
            "Garage preparation cannot materialize preset '{}'",
            service.desired().preset().unwrap_or("<none>")
        )));
    }

    let project_id = service.project().as_str();
    let service_id = service.service().as_str();
    let container_name = format!("stackctl-{project_id}-{service_id}");
    let bucket = container_name.clone();
    if bucket.len() > 63 {
        return Err(invalid(format!(
            "Garage bucket '{bucket}' exceeds the 63-byte S3 limit"
        )));
    }
    let command = GENERATED_COMMAND.map(str::to_owned).to_vec();
    if service
        .desired()
        .command()
        .is_some_and(|declared| declared != command)
    {
        return Err(invalid(format!(
            "Garage service '{project_id}-{service_id}' cannot replace its generated command"
        )));
    }

    let access_key = garage_access_key(project_id, service_id);
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("{project_id}/{service_id}/garage"),
        project_id: Some(project_id.to_owned()),
        service_id: service_id.to_owned(),
        username: access_key.clone(),
        secret: secret.expose().to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let container_environment = BTreeMap::from([
        (
            "GARAGE_CONFIG_FILE".to_owned(),
            "/etc/garage.toml".to_owned(),
        ),
        ("GARAGE_DEFAULT_ACCESS_KEY".to_owned(), access_key.clone()),
        ("GARAGE_DEFAULT_BUCKET".to_owned(), bucket.clone()),
        (
            "GARAGE_DEFAULT_SECRET_KEY".to_owned(),
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
                "Garage service '{project_id}-{service_id}' cannot replace generated \
                 environment key '{key}'"
            )));
        }
    }

    let values = BTreeMap::from([
        ("AWS_ACCESS_KEY_ID".to_owned(), access_key),
        ("AWS_BUCKET".to_owned(), bucket),
        ("AWS_DEFAULT_REGION".to_owned(), "garage".to_owned()),
        (
            "AWS_ENDPOINT".to_owned(),
            format!("http://{container_name}:3900"),
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
    let configuration = ProjectServiceContainerConfiguration::new(
        "garage.toml",
        "/etc/garage.toml",
        CredentialSecret::new(garage_configuration(&container_name, &secret)),
    )?;

    PreparedProjectService::new(
        project_id.to_owned(),
        service_id.to_owned(),
        Some(credential),
        environment,
        container_environment,
        None,
    )
    .with_container_command(command)
    .map(|prepared| prepared.with_container_configuration(configuration))
}

fn garage_access_key(project_id: &str, service_id: &str) -> String {
    let identity = format!("{project_id}/{service_id}");
    format!("GK{}", &hex::encode(Sha256::digest(identity))[..32])
}

fn garage_configuration(container_name: &str, secret: &CredentialSecret) -> String {
    let rpc_secret = derived_secret("garage-rpc", secret);
    let admin_token = derived_secret("garage-admin", secret);
    let metrics_token = derived_secret("garage-metrics", secret);

    format!(
        concat!(
            "metadata_dir = \"/var/lib/garage/meta\"\n",
            "data_dir = \"/var/lib/garage/data\"\n",
            "db_engine = \"sqlite\"\n",
            "replication_factor = 1\n",
            "rpc_bind_addr = \"[::]:3901\"\n",
            "rpc_public_addr = \"{container_name}:3901\"\n",
            "rpc_secret = \"{rpc_secret}\"\n\n",
            "[s3_api]\n",
            "s3_region = \"garage\"\n",
            "api_bind_addr = \"[::]:3900\"\n",
            "root_domain = \".s3.garage.localhost\"\n\n",
            "[admin]\n",
            "api_bind_addr = \"[::]:3903\"\n",
            "admin_token = \"{admin_token}\"\n",
            "metrics_token = \"{metrics_token}\"\n"
        ),
        container_name = container_name,
        rpc_secret = rpc_secret,
        admin_token = admin_token,
        metrics_token = metrics_token,
    )
}

fn derived_secret(domain: &str, secret: &CredentialSecret) -> String {
    let mut digest = Sha256::new();
    digest.update(domain.as_bytes());
    digest.update([0]);
    digest.update(secret.expose().as_bytes());

    hex::encode(digest.finalize())
}

fn invalid(error: impl std::fmt::Display) -> ProjectServicePreparationError {
    ProjectServicePreparationError::new(error.to_string())
}
