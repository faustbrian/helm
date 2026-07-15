use super::{
    PreparedProjectService, materialize_project_service_configurations,
    plan_garage_project_resources, plan_localstack_project_resources,
};
use crate::control_plane::application::{ProjectSource, plan_project_registry};
use crate::control_plane::engine::{
    BollardEngineAdapter, ContainerCreateOptions, ContainerLifecycle, ImageResolver,
    ImmutableImageReference, InstallationResourceDeletionOptions, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, NetworkCreateOptions, NetworkManager, ResourceKind,
    RetentionClass, VolumeManager, delete_owned_installation_resources,
};
use crate::control_plane::shared_infrastructure::{
    CredentialSecret, ProvisioningJobOptions, SharedInfrastructureReconcileError,
    run_provisioning_job,
};
use crate::control_plane::workload::{
    DedicatedProjectServiceOptions, DedicatedProjectServicePlan, plan_dedicated_project_service,
};
use crate::control_plane::{ServiceExecutionPlan, resolve_execution_plan};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const GARAGE_IMAGE: &str = concat!(
    "dxflrs/garage@sha256:",
    "866bd13ed2038ba7e7190e840482bc27234c4afaf77be8cfa439ae088c1e4690"
);
const LOCALSTACK_IMAGE: &str = concat!(
    "localstack/localstack@sha256:",
    "3ebc37595918b8accb852f8048fef2aff047d465167edd655528065b07bc364a"
);

#[test]
#[ignore = "CI owns live dedicated object-store readiness acceptance"]
fn live_docker_engine_dedicated_object_stores_provision_idempotent_buckets() {
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
    let state_directory = std::env::temp_dir().join(format!("stackctl-{installation_id}"));
    let source = ProjectSource::new(
        PathBuf::from(format!("/work/{project_id}")),
        PathBuf::from(format!("/work/{project_id}/.stackctl.yaml")),
        format!(
            concat!(
                "schema_version: 8\nproject: {project}\nservices:\n",
                "  storage:\n    preset: garage\n    version: '2'\n",
                "    image: {garage_image}\n",
                "  cloud:\n    preset: localstack\n    version: '4'\n",
                "    image: {localstack_image}\n"
            ),
            project = project_id,
            garage_image = GARAGE_IMAGE,
            localstack_image = LOCALSTACK_IMAGE,
        ),
    );
    let registry = plan_project_registry(&[source]).expect("plan dedicated object-store registry");
    let execution =
        resolve_execution_plan(&registry).expect("resolve dedicated object-store execution");
    let garage = service(&execution, "storage");
    let localstack = service(&execution, "cloud");
    let garage_secret = format!("garage-{nonce}-secret");
    let mut prepared = vec![
        plan_garage_project_resources(garage, CredentialSecret::new(garage_secret.clone()))
            .expect("prepare dedicated Garage"),
        plan_localstack_project_resources(localstack).expect("prepare dedicated LocalStack"),
    ];
    materialize_project_service_configurations(&mut prepared, &state_directory)
        .expect("materialize Garage configuration");
    let platform = linux_platform();
    let garage_plan = plan_service(
        garage,
        &prepared[0],
        &installation_id,
        &network_name,
        platform,
    );
    let localstack_plan = plan_service(
        localstack,
        &prepared[1],
        &installation_id,
        &network_name,
        platform,
    );
    let wrong_garage = plan_garage_project_resources(
        garage,
        CredentialSecret::new(format!("wrong-{garage_secret}")),
    )
    .expect("prepare incorrect Garage credential");
    let wrong_garage_plan = plan_service(
        garage,
        &wrong_garage,
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
    .expect("build dedicated object-store network metadata");
    let network = NetworkCreateOptions::new(&network_name, network_metadata)
        .expect("build dedicated object-store network request");
    let plans = [garage_plan, localstack_plan];
    let bucket_names = [
        format!("garage/stackctl-{project_id}-storage"),
        format!("localstack/stackctl-{project_id}-cloud"),
    ];
    let remove_bucket_jobs = plans
        .iter()
        .zip(&bucket_names)
        .map(|(plan, bucket)| {
            object_store_client_job(
                plan.provisioning_job()
                    .expect("object-store provisioning request"),
                &installation_id,
                &project_id,
                format!(
                    "{}-remove",
                    plan.request().metadata().resource_id().unwrap_or_default()
                ),
                vec!["rb".to_owned(), "--force".to_owned(), bucket.clone()],
            )
        })
        .collect::<Vec<_>>();
    let stat_bucket_jobs = plans
        .iter()
        .zip(&bucket_names)
        .map(|(plan, bucket)| {
            object_store_client_job(
                plan.provisioning_job()
                    .expect("object-store provisioning request"),
                &installation_id,
                &project_id,
                format!(
                    "{}-stat",
                    plan.request().metadata().resource_id().unwrap_or_default()
                ),
                vec!["stat".to_owned(), bucket.clone()],
            )
        })
        .collect::<Vec<_>>();
    let retained_volumes = plans
        .iter()
        .map(|plan| {
            plan.volume()
                .expect("retained object-store volume")
                .name()
                .to_owned()
        })
        .collect::<Vec<_>>();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build dedicated object-store acceptance runtime");
    let mut engine = runtime
        .block_on(BollardEngineAdapter::connect_unix(&socket))
        .expect("negotiate the selected Docker Engine API");

    let acceptance = runtime.block_on(async {
        engine
            .create_network(&network)
            .await
            .map_err(|error| engine_error("create dedicated object-store network", error))?;
        for plan in &plans {
            let image = ImmutableImageReference::new(plan.request().image())
                .map_err(|error| engine_error("validate dedicated object-store image", error))?;
            engine
                .ensure_image(&image)
                .await
                .map_err(|error| engine_error("resolve dedicated object-store image", error))?;
            engine
                .create_volume(plan.volume().expect("retained object-store volume"))
                .await
                .map_err(|error| engine_error("create dedicated object-store volume", error))?;
            let container = engine
                .create(plan.request())
                .await
                .map_err(|error| engine_error("create dedicated object-store container", error))?;
            engine
                .start(&container)
                .await
                .map_err(|error| engine_error("start dedicated object-store container", error))?;
        }
        for plan in &plans {
            probe_until_ready(
                &mut engine,
                plan.provisioning_job()
                    .expect("object-store bucket provisioning job"),
                &installation_id,
            )
            .await?;
        }
        assert_job_fails(
            &mut engine,
            wrong_garage_plan
                .provisioning_job()
                .expect("incorrect Garage provisioning job"),
            &installation_id,
            "incorrect Garage credential unexpectedly provisioned a bucket",
        )
        .await?;
        for request in &remove_bucket_jobs {
            run_job(&mut engine, request, &installation_id).await?;
        }
        for request in &stat_bucket_jobs {
            assert_job_fails(
                &mut engine,
                request,
                &installation_id,
                "removed object-store bucket unexpectedly remained present",
            )
            .await?;
        }
        for plan in &plans {
            probe_until_ready(
                &mut engine,
                plan.provisioning_job()
                    .expect("idempotent object-store bucket provisioning job"),
                &installation_id,
            )
            .await?;
        }
        for request in &stat_bucket_jobs {
            run_job(&mut engine, request, &installation_id).await?;
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
        .expect("delete dedicated object-store acceptance resources");
    std::fs::remove_dir_all(&state_directory)
        .expect("delete dedicated object-store generated configuration");
    acceptance.expect("run dedicated object-store provisioning jobs");

    println!("dedicated object-store readiness passed for {installation_id}");
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
    .expect("plan dedicated object-store service")
}

fn object_store_client_job(
    template: &ContainerCreateOptions,
    installation_id: &str,
    project_id: &str,
    resource_id: String,
    command: Vec<String>,
) -> ContainerCreateOptions {
    let manifest = serde_json::to_vec(&(
        template.image(),
        template.platform(),
        template.network(),
        &command,
        template.environment(),
    ))
    .expect("serialize object-store client job");
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.to_owned(),
        kind: ResourceKind::ProvisioningJob,
        project_id: Some(project_id.to_owned()),
        compatibility_fingerprint: format!(
            "sha256:{}",
            hex::encode(Sha256::digest(template.image()))
        ),
        schema_version: 8,
        desired_revision: format!("sha256:{}", hex::encode(Sha256::digest(manifest))),
        retention: RetentionClass::Disposable,
    })
    .and_then(|metadata| metadata.with_resource_id(resource_id.clone()))
    .expect("build object-store client metadata");

    ContainerCreateOptions::new(
        format!("stackctl-job-{project_id}-{resource_id}"),
        template.image(),
        metadata,
    )
    .and_then(|request| request.with_platform(template.platform().expect("client job platform")))
    .and_then(|request| request.with_network(template.network().expect("client job network")))
    .and_then(|request| request.with_command(command))
    .and_then(|request| request.with_environment(template.environment().clone()))
    .expect("build object-store client request")
}

async fn assert_job_fails(
    engine: &mut BollardEngineAdapter,
    request: &ContainerCreateOptions,
    installation_id: &str,
    unexpected_success: &str,
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
        Err(SharedInfrastructureReconcileError::ProvisioningFailed { .. }) => Ok(()),
        Err(error) => Err(SharedInfrastructureReconcileError::InvalidRequest {
            detail: format!("expected object-store client failure was not classified: {error}"),
        }),
        Ok(()) => Err(SharedInfrastructureReconcileError::InvalidRequest {
            detail: unexpected_success.to_owned(),
        }),
    }
}

async fn run_job(
    engine: &mut BollardEngineAdapter,
    request: &ContainerCreateOptions,
    installation_id: &str,
) -> Result<(), SharedInfrastructureReconcileError> {
    run_provisioning_job(
        engine,
        ProvisioningJobOptions {
            request,
            installation_id,
            schema_version: 8,
            timeout: Duration::from_secs(10),
        },
    )
    .await
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
    request: &ContainerCreateOptions,
    installation_id: &str,
) -> Result<(), SharedInfrastructureReconcileError> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
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
        .expect("dedicated object-store execution service")
}

fn linux_platform() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "linux/amd64",
        "aarch64" => "linux/arm64",
        architecture => panic!("unsupported Engine acceptance architecture '{architecture}'"),
    }
}
