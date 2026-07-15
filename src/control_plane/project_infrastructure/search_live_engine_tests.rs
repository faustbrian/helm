use super::{plan_meilisearch_project_resources, plan_typesense_project_resources};
use crate::control_plane::application::{ProjectSource, plan_project_registry};
use crate::control_plane::engine::{
    BollardEngineAdapter, ContainerLifecycle, ImageResolver, ImmutableImageReference,
    InstallationResourceDeletionOptions, ManagedResourceMetadata, ManagedResourceMetadataOptions,
    NetworkCreateOptions, NetworkManager, ResourceKind, RetentionClass, VolumeManager,
    delete_owned_installation_resources,
};
use crate::control_plane::shared_infrastructure::{
    CredentialSecret, ProvisioningJobOptions, SharedInfrastructureReconcileError,
    run_provisioning_job,
};
use crate::control_plane::workload::{
    DedicatedProjectServiceOptions, DedicatedProjectServicePlan, plan_dedicated_project_service,
};
use crate::control_plane::{ServiceExecutionPlan, resolve_execution_plan};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MEILISEARCH_IMAGE: &str = concat!(
    "getmeili/meilisearch@sha256:",
    "ac40212f9e5a7526d8007586e3e46fb0441d29dd36c7b02fa2341d2c9a1f6493"
);
const TYPESENSE_IMAGE: &str = concat!(
    "typesense/typesense@sha256:",
    "f8a9d59c8ceaf67e547bac03a1df74db9b806abfcd497a6d7d6c8d9d8eef5f20"
);

#[test]
#[ignore = "CI owns live dedicated search readiness acceptance"]
fn live_docker_engine_search_services_require_authenticated_readiness() {
    let socket = std::env::var_os("STACKCTL_ENGINE_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/run/docker.sock"));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must follow the Unix epoch")
        .as_nanos();
    let suffix = nonce % 1_000_000_000;
    let project_id = format!("ci-{}-{suffix}", std::process::id());
    let installation_id = format!("{project_id}-install");
    let network_name = format!("stackctl-{project_id}");
    let source = ProjectSource::new(
        PathBuf::from(format!("/work/{project_id}")),
        PathBuf::from(format!("/work/{project_id}/.stackctl.yaml")),
        format!(
            concat!(
                "schema_version: 8\nproject: {project}\nservices:\n",
                "  documents:\n    preset: meilisearch\n    version: '1'\n",
                "    image: {meilisearch_image}\n",
                "  catalogue:\n    preset: typesense\n    version: '26'\n",
                "    image: {typesense_image}\n"
            ),
            project = project_id,
            meilisearch_image = MEILISEARCH_IMAGE,
            typesense_image = TYPESENSE_IMAGE,
        ),
    );
    let registry = plan_project_registry(&[source]).expect("plan dedicated search registry");
    let execution = resolve_execution_plan(&registry).expect("resolve dedicated search execution");
    let meilisearch = service(&execution, "documents");
    let typesense = service(&execution, "catalogue");
    let meilisearch_secret = format!("meili-{nonce}-secret");
    let typesense_secret = format!("typesense-{nonce}-secret");
    let prepared_meilisearch = plan_meilisearch_project_resources(
        meilisearch,
        CredentialSecret::new(meilisearch_secret.clone()),
    )
    .expect("prepare dedicated Meilisearch");
    let prepared_typesense = plan_typesense_project_resources(
        typesense,
        CredentialSecret::new(typesense_secret.clone()),
    )
    .expect("prepare dedicated Typesense");
    let platform = linux_platform();
    let meilisearch_plan = plan_service(
        meilisearch,
        &prepared_meilisearch,
        &installation_id,
        &network_name,
        platform,
    );
    let typesense_plan = plan_service(
        typesense,
        &prepared_typesense,
        &installation_id,
        &network_name,
        platform,
    );
    let wrong_meilisearch = plan_meilisearch_project_resources(
        meilisearch,
        CredentialSecret::new(format!("wrong-{meilisearch_secret}")),
    )
    .expect("prepare incorrect Meilisearch credential");
    let wrong_typesense = plan_typesense_project_resources(
        typesense,
        CredentialSecret::new(format!("wrong-{typesense_secret}")),
    )
    .expect("prepare incorrect Typesense credential");
    let wrong_meilisearch_plan = plan_service(
        meilisearch,
        &wrong_meilisearch,
        &installation_id,
        &network_name,
        platform,
    );
    let wrong_typesense_plan = plan_service(
        typesense,
        &wrong_typesense,
        &installation_id,
        &network_name,
        platform,
    );
    let network_metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.clone(),
        kind: ResourceKind::Network,
        project_id: None,
        compatibility_fingerprint: format!("sha256:{}", "a".repeat(64)),
        schema_version: 8,
        desired_revision: format!("sha256:{}", "b".repeat(64)),
        retention: RetentionClass::Persistent,
    })
    .expect("build dedicated search network metadata");
    let network = NetworkCreateOptions::new(&network_name, network_metadata)
        .expect("build dedicated search network request");
    let plans = [meilisearch_plan, typesense_plan];
    let retained_volumes = plans
        .iter()
        .map(|plan| {
            plan.volume()
                .expect("retained search volume")
                .name()
                .to_owned()
        })
        .collect::<Vec<_>>();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build dedicated search acceptance runtime");
    let mut engine = runtime
        .block_on(BollardEngineAdapter::connect_unix(&socket))
        .expect("negotiate the selected Docker Engine API");

    let acceptance = runtime.block_on(async {
        engine
            .create_network(&network)
            .await
            .map_err(|error| engine_error("create dedicated search network", error))?;
        for plan in &plans {
            let image = ImmutableImageReference::new(plan.request().image())
                .map_err(|error| engine_error("validate dedicated search image", error))?;
            engine
                .ensure_image(&image)
                .await
                .map_err(|error| engine_error("resolve dedicated search image", error))?;
            engine
                .create_volume(plan.volume().expect("retained search volume"))
                .await
                .map_err(|error| engine_error("create dedicated search volume", error))?;
            let container = engine
                .create(plan.request())
                .await
                .map_err(|error| engine_error("create dedicated search container", error))?;
            engine
                .start(&container)
                .await
                .map_err(|error| engine_error("start dedicated search container", error))?;
        }
        for plan in &plans {
            probe_until_ready(
                &mut engine,
                plan.provisioning_job()
                    .expect("authenticated search readiness job"),
                &installation_id,
            )
            .await?;
        }
        assert_authentication_rejected(
            &mut engine,
            wrong_meilisearch_plan
                .provisioning_job()
                .expect("incorrect Meilisearch readiness job"),
            &installation_id,
        )
        .await?;
        assert_authentication_rejected(
            &mut engine,
            wrong_typesense_plan
                .provisioning_job()
                .expect("incorrect Typesense readiness job"),
            &installation_id,
        )
        .await?;

        Ok::<_, SharedInfrastructureReconcileError>(())
    });
    runtime
        .block_on(delete_owned_installation_resources(
            &mut engine,
            InstallationResourceDeletionOptions {
                installation_id: &installation_id,
                schema_version: 8,
                authorized_persistent_volumes: &retained_volumes,
            },
        ))
        .expect("delete dedicated search acceptance resources");
    acceptance.expect("run authenticated search readiness jobs");

    println!("dedicated search readiness passed for {installation_id}");
}

fn plan_service<'service>(
    service: &'service ServiceExecutionPlan,
    prepared: &'service super::PreparedProjectService,
    installation_id: &str,
    network_name: &str,
    platform: &str,
) -> DedicatedProjectServicePlan {
    plan_dedicated_project_service(DedicatedProjectServiceOptions {
        service,
        generated_environment: Some(prepared.container_environment()),
        generated_command: prepared.container_command(),
        generated_configuration_mount: prepared.container_configuration_mount(),
        provisioning_job: prepared.provisioning_job(),
        installation_id,
        schema_version: 8,
        platform,
        network_name,
    })
    .expect("plan dedicated search service")
}

async fn assert_authentication_rejected(
    engine: &mut BollardEngineAdapter,
    request: &crate::control_plane::engine::ContainerCreateOptions,
    installation_id: &str,
) -> Result<(), SharedInfrastructureReconcileError> {
    match run_provisioning_job(
        engine,
        ProvisioningJobOptions {
            request,
            installation_id,
            schema_version: 8,
            timeout: Duration::from_secs(10),
        },
    )
    .await
    {
        Err(SharedInfrastructureReconcileError::ProvisioningFailed {
            status_code: 42, ..
        }) => Ok(()),
        Err(error) => Err(SharedInfrastructureReconcileError::InvalidRequest {
            detail: format!(
                "incorrect search credential was not classified as authentication failure: {error}"
            ),
        }),
        Ok(()) => Err(SharedInfrastructureReconcileError::InvalidRequest {
            detail: "incorrect search credential unexpectedly passed readiness".to_owned(),
        }),
    }
}

fn engine_error(
    action: &str,
    error: crate::control_plane::engine::EngineError,
) -> SharedInfrastructureReconcileError {
    SharedInfrastructureReconcileError::Engine {
        action: action.to_owned(),
        detail: error.to_string(),
    }
}

async fn probe_until_ready(
    engine: &mut BollardEngineAdapter,
    request: &crate::control_plane::engine::ContainerCreateOptions,
    installation_id: &str,
) -> Result<(), SharedInfrastructureReconcileError> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        match run_provisioning_job(
            engine,
            ProvisioningJobOptions {
                request,
                installation_id,
                schema_version: 8,
                timeout: Duration::from_secs(10),
            },
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(error @ SharedInfrastructureReconcileError::ProvisioningFailed { .. })
                if tokio::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(Duration::from_millis(250)).await;
                drop(error);
            }
            Err(error) => return Err(error),
        }
    }
}

fn service<'execution>(
    execution: &'execution crate::control_plane::ExecutionPlan,
    service_id: &str,
) -> &'execution ServiceExecutionPlan {
    execution
        .services()
        .iter()
        .find(|service| service.service().as_str() == service_id)
        .expect("dedicated search execution service")
}

fn linux_platform() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "linux/amd64",
        "aarch64" => "linux/arm64",
        architecture => panic!("unsupported Engine acceptance architecture '{architecture}'"),
    }
}
