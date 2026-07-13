use super::{ObjectStoreFlavor, ObjectStoreProjectResources, ObjectStoreSharedInstancePlan};
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, OwnedContainer,
    run_attached_command,
};
use std::collections::BTreeMap;
use std::time::Duration;

const PROVISION_TIMEOUT_SECONDS: u64 = 30;
const ALIAS: &str = "stackctl";

/// Reconciles one MinIO bucket, identity, and bucket-scoped policy.
pub(crate) async fn provision_object_store_project_resources(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    instance: &ObjectStoreSharedInstancePlan,
    project: &ObjectStoreProjectResources,
) -> Result<(), EngineError> {
    if instance.flavor() != ObjectStoreFlavor::Minio {
        return Err(EngineError::InvalidRequest {
            detail: "RustFS IAM provisioning is not proven; refusing MinIO command assumptions"
                .to_owned(),
        });
    }

    let definition = project.definition();
    let environment = BTreeMap::from([(
        "MC_HOST_stackctl".to_owned(),
        format!(
            "http://{}:{}@127.0.0.1:9000",
            instance.root_credential().username(),
            instance.root_credential().secret()
        ),
    )]);
    run(
        executor,
        container,
        vec![
            "mc".to_owned(),
            "mb".to_owned(),
            "--ignore-existing".to_owned(),
            format!("{ALIAS}/{}", definition.bucket()),
        ],
        environment.clone(),
        Vec::new(),
        "create MinIO project bucket",
    )
    .await?;
    run(
        executor,
        container,
        vec![
            "mc".to_owned(),
            "admin".to_owned(),
            "policy".to_owned(),
            "create".to_owned(),
            ALIAS.to_owned(),
            definition.policy_name().to_owned(),
            instance.policy_file(definition.policy_name()),
        ],
        environment.clone(),
        Vec::new(),
        "publish MinIO project policy",
    )
    .await?;
    run(
        executor,
        container,
        vec![
            "mc".to_owned(),
            "admin".to_owned(),
            "user".to_owned(),
            "add".to_owned(),
            ALIAS.to_owned(),
        ],
        environment.clone(),
        format!(
            "{}\n{}\n",
            definition.username(),
            definition.secret().expose()
        )
        .into_bytes(),
        "create MinIO project identity",
    )
    .await?;
    run(
        executor,
        container,
        vec![
            "mc".to_owned(),
            "admin".to_owned(),
            "policy".to_owned(),
            "attach".to_owned(),
            ALIAS.to_owned(),
            definition.policy_name().to_owned(),
            "--user".to_owned(),
            definition.username().to_owned(),
        ],
        environment,
        Vec::new(),
        "attach MinIO project policy",
    )
    .await
}

async fn run(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    arguments: Vec<String>,
    environment: BTreeMap<String, String>,
    input: Vec<u8>,
    action: &str,
) -> Result<(), EngineError> {
    let request = CommandRequest::new(arguments, environment, None)?;
    let options = AttachedCommandOptions::new(
        request,
        input,
        action,
        Duration::from_secs(PROVISION_TIMEOUT_SECONDS),
    )?;

    run_attached_command(executor, container, &options).await
}
