use super::{
    PreparedProjectService, opensearch_initial_admin_password,
    plan_elasticsearch_project_resources, plan_opensearch_project_resources,
};
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

const ELASTICSEARCH_IMAGE: &str = concat!(
    "docker.elastic.co/elasticsearch/elasticsearch@sha256:",
    "be5f49784ff5ec8a5b5d7ba17f944d9d6b10c067f596ee93e6b6cb82d2dd874c"
);
const OPENSEARCH_IMAGE: &str = concat!(
    "opensearchproject/opensearch@sha256:",
    "b5dd1512af2a99748c942cfbbd7f32162623336b210667d0fc6333c6321f171d"
);

#[test]
#[ignore = "CI owns live Elasticsearch and OpenSearch acceptance"]
fn live_docker_engine_enterprise_search_recovers_with_stable_authentication() {
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
                "  elastic:\n    preset: elasticsearch\n    version: '9'\n",
                "    image: {elasticsearch_image}\n",
                "  open:\n    preset: opensearch\n    version: '3'\n",
                "    image: {opensearch_image}\n"
            ),
            project = project_id,
            elasticsearch_image = ELASTICSEARCH_IMAGE,
            opensearch_image = OPENSEARCH_IMAGE,
        ),
    );
    let registry = plan_project_registry(&[source]).expect("plan enterprise search registry");
    let execution = resolve_execution_plan(&registry).expect("resolve enterprise search execution");
    let elasticsearch = service(&execution, "elastic");
    let opensearch = service(&execution, "open");
    let elasticsearch_secret = format!("elastic-{nonce}-secret");
    let opensearch_secret =
        opensearch_initial_admin_password(CredentialSecret::new(format!("open-{nonce}-secret")));
    let prepared_elasticsearch = plan_elasticsearch_project_resources(
        elasticsearch,
        CredentialSecret::new(elasticsearch_secret.clone()),
    )
    .expect("prepare Elasticsearch");
    let prepared_opensearch =
        plan_opensearch_project_resources(opensearch, opensearch_secret.clone())
            .expect("prepare OpenSearch");
    let platform = linux_platform();
    let elasticsearch_plan = plan_service(
        elasticsearch,
        &prepared_elasticsearch,
        &installation_id,
        &network_name,
        platform,
    );
    let opensearch_plan = plan_service(
        opensearch,
        &prepared_opensearch,
        &installation_id,
        &network_name,
        platform,
    );
    let wrong_elasticsearch = plan_elasticsearch_project_resources(
        elasticsearch,
        CredentialSecret::new(format!("wrong-{elasticsearch_secret}")),
    )
    .expect("prepare incorrect Elasticsearch credential");
    let wrong_opensearch = plan_opensearch_project_resources(
        opensearch,
        opensearch_initial_admin_password(CredentialSecret::new(format!(
            "wrong-{}",
            opensearch_secret.expose()
        ))),
    )
    .expect("prepare incorrect OpenSearch credential");
    let wrong_elasticsearch_plan = plan_service(
        elasticsearch,
        &wrong_elasticsearch,
        &installation_id,
        &network_name,
        platform,
    );
    let wrong_opensearch_plan = plan_service(
        opensearch,
        &wrong_opensearch,
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
    .expect("build enterprise search network metadata");
    let network = NetworkCreateOptions::new(&network_name, network_metadata)
        .expect("build enterprise search network request");
    let plans = [
        (&elasticsearch_plan, &wrong_elasticsearch_plan),
        (&opensearch_plan, &wrong_opensearch_plan),
    ];
    let retained_volumes = plans
        .iter()
        .map(|(plan, _)| {
            plan.volume()
                .expect("retained enterprise search volume")
                .name()
                .to_owned()
        })
        .collect::<Vec<_>>();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build enterprise search acceptance runtime");
    let mut engine = runtime
        .block_on(BollardEngineAdapter::connect_unix(&socket))
        .expect("negotiate the selected Docker Engine API");

    let acceptance = runtime.block_on(async {
        engine
            .create_network(&network)
            .await
            .map_err(|error| engine_error("create enterprise search network", error))?;
        for (plan, wrong_plan) in plans {
            verify_service_recovery(&mut engine, plan, wrong_plan, &installation_id).await?;
        }

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
        .expect("delete enterprise search acceptance resources");
    acceptance.expect("verify enterprise search authentication and recovery");

    println!("enterprise search acceptance passed for {installation_id}");
}

async fn verify_service_recovery(
    engine: &mut BollardEngineAdapter,
    plan: &DedicatedProjectServicePlan,
    wrong_plan: &DedicatedProjectServicePlan,
    installation_id: &str,
) -> Result<(), SharedInfrastructureReconcileError> {
    let image = ImmutableImageReference::new(plan.request().image())
        .map_err(|error| engine_error("validate enterprise search image", error))?;
    engine
        .ensure_image(&image)
        .await
        .map_err(|error| engine_error("resolve enterprise search image", error))?;
    engine
        .create_volume(plan.volume().expect("retained enterprise search volume"))
        .await
        .map_err(|error| engine_error("create enterprise search volume", error))?;
    let original = engine
        .create(plan.request())
        .await
        .map_err(|error| engine_error("create enterprise search container", error))?;
    engine
        .start(&original)
        .await
        .map_err(|error| engine_error("start enterprise search container", error))?;
    probe_until_ready(
        engine,
        plan.provisioning_job()
            .expect("authenticated enterprise search readiness job"),
        installation_id,
    )
    .await?;
    assert_authentication_rejected(
        engine,
        wrong_plan
            .provisioning_job()
            .expect("incorrect enterprise search readiness job"),
        installation_id,
    )
    .await?;
    engine
        .stop(&original)
        .await
        .map_err(|error| engine_error("stop enterprise search container", error))?;
    engine
        .remove(&original)
        .await
        .map_err(|error| engine_error("remove enterprise search container", error))?;
    let replacement = engine
        .create(plan.request())
        .await
        .map_err(|error| engine_error("recreate enterprise search container", error))?;
    if replacement.id() == original.id() {
        return Err(SharedInfrastructureReconcileError::InvalidRequest {
            detail: "enterprise search replacement reused the removed container ID".to_owned(),
        });
    }
    engine
        .start(&replacement)
        .await
        .map_err(|error| engine_error("start replacement search container", error))?;
    probe_until_ready(
        engine,
        plan.provisioning_job()
            .expect("replacement authenticated readiness job"),
        installation_id,
    )
    .await
}

fn plan_service<'service>(
    service: &'service ServiceExecutionPlan,
    prepared: &'service PreparedProjectService,
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
    .expect("plan enterprise search service")
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
            timeout: Duration::from_secs(15),
        },
    )
    .await
    {
        Err(SharedInfrastructureReconcileError::ProvisioningFailed {
            status_code: 42, ..
        }) => Ok(()),
        Err(error) => Err(SharedInfrastructureReconcileError::InvalidRequest {
            detail: format!(
                "incorrect enterprise search credential was not classified as authentication \
                 failure: {error}"
            ),
        }),
        Ok(()) => Err(SharedInfrastructureReconcileError::InvalidRequest {
            detail: "incorrect enterprise search credential unexpectedly passed readiness"
                .to_owned(),
        }),
    }
}

async fn probe_until_ready(
    engine: &mut BollardEngineAdapter,
    request: &crate::control_plane::engine::ContainerCreateOptions,
    installation_id: &str,
) -> Result<(), SharedInfrastructureReconcileError> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    loop {
        match run_provisioning_job(
            engine,
            ProvisioningJobOptions {
                request,
                installation_id,
                schema_version: 8,
                timeout: Duration::from_secs(15),
            },
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(error @ SharedInfrastructureReconcileError::ProvisioningFailed { .. })
                if tokio::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(Duration::from_millis(500)).await;
                drop(error);
            }
            Err(error) => return Err(error),
        }
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

fn service<'execution>(
    execution: &'execution crate::control_plane::ExecutionPlan,
    service_id: &str,
) -> &'execution ServiceExecutionPlan {
    execution
        .services()
        .iter()
        .find(|service| service.service().as_str() == service_id)
        .expect("enterprise search execution service")
}

fn linux_platform() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "linux/amd64",
        "aarch64" => "linux/arm64",
        architecture => panic!("unsupported Engine acceptance architecture '{architecture}'"),
    }
}
