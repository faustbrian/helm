use super::{plan_dragonfly_project_resources, plan_memcached_project_resources};
use crate::control_plane::application::{ProjectSource, plan_project_registry};
use crate::control_plane::engine::{
    BollardEngineAdapter, InstallationResourceDeletionOptions, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, NetworkCreateOptions, NetworkManager, ResourceKind,
    RetentionClass, delete_owned_installation_resources,
};
use crate::control_plane::shared_infrastructure::{
    CredentialSecret, ProvisioningJobOptions, SharedInfrastructureReconcileError,
    run_provisioning_job,
};
use crate::control_plane::workload::{
    DedicatedProjectServiceOptions, ProjectVolumeReconcileOptions, WorkloadReconcileOptions,
    plan_dedicated_project_service, reconcile_project_service, reconcile_project_volume,
};
use crate::control_plane::{ServiceExecutionPlan, resolve_execution_plan};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DRAGONFLY_IMAGE: &str = concat!(
    "docker.dragonflydb.io/dragonflydb/dragonfly@sha256:",
    "0fa01a2b929e704c7a9300d23e7f52002ebd39e90996fb8bb63826aed92fa06f"
);
const MEMCACHED_IMAGE: &str = concat!(
    "memcached@sha256:",
    "c29847751abb41f4c268c84fb3087fee05d4edcbda44409ccb5086e26148e8a7"
);

#[test]
#[ignore = "CI owns live dedicated cache readiness acceptance"]
fn live_docker_engine_dedicated_caches_pass_protocol_readiness() {
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
                "  cache:\n    preset: dragonfly\n    version: '1'\n",
                "    image: {dragonfly_image}\n",
                "  memory:\n    preset: memcached\n    version: '1'\n",
                "    image: {memcached_image}\n"
            ),
            project = project_id,
            dragonfly_image = DRAGONFLY_IMAGE,
            memcached_image = MEMCACHED_IMAGE,
        ),
    );
    let registry = plan_project_registry(&[source]).expect("plan dedicated cache registry");
    let execution = resolve_execution_plan(&registry).expect("resolve dedicated cache execution");
    let dragonfly = service(&execution, "cache");
    let memcached = service(&execution, "memory");
    let password = format!("stackctl-{nonce}-secret");
    let prepared_dragonfly =
        plan_dragonfly_project_resources(dragonfly, CredentialSecret::new(password))
            .expect("prepare dedicated Dragonfly");
    let prepared_memcached =
        plan_memcached_project_resources(memcached).expect("prepare dedicated Memcached");
    let platform = linux_platform();
    let dragonfly_plan = plan_dedicated_project_service(DedicatedProjectServiceOptions {
        service: dragonfly,
        generated_environment: Some(prepared_dragonfly.container_environment()),
        generated_command: prepared_dragonfly.container_command(),
        generated_configuration_mount: prepared_dragonfly.container_configuration_mount(),
        provisioning_job: prepared_dragonfly.provisioning_job(),
        installation_id: &installation_id,
        schema_version: 8,
        platform,
        network_name: &network_name,
    })
    .expect("plan dedicated Dragonfly");
    let memcached_plan = plan_dedicated_project_service(DedicatedProjectServiceOptions {
        service: memcached,
        generated_environment: Some(prepared_memcached.container_environment()),
        generated_command: prepared_memcached.container_command(),
        generated_configuration_mount: prepared_memcached.container_configuration_mount(),
        provisioning_job: prepared_memcached.provisioning_job(),
        installation_id: &installation_id,
        schema_version: 8,
        platform,
        network_name: &network_name,
    })
    .expect("plan dedicated Memcached");
    let network_metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.clone(),
        kind: ResourceKind::Network,
        project_id: None,
        compatibility_fingerprint: format!("sha256:{}", "a".repeat(64)),
        schema_version: 8,
        desired_revision: format!("sha256:{}", "b".repeat(64)),
        retention: RetentionClass::Persistent,
    })
    .expect("build dedicated cache network metadata");
    let network = NetworkCreateOptions::new(&network_name, network_metadata)
        .expect("build dedicated cache network request");
    let dragonfly_volume = dragonfly_plan
        .volume()
        .expect("Dragonfly retained volume")
        .name()
        .to_owned();
    let plans = [dragonfly_plan, memcached_plan];
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build dedicated cache acceptance runtime");
    let mut engine = runtime
        .block_on(BollardEngineAdapter::connect_unix(&socket))
        .expect("negotiate the selected Docker Engine API");

    let acceptance = runtime.block_on(async {
        engine
            .create_network(&network)
            .await
            .map_err(|error| engine_error("create dedicated cache network", error))?;
        for plan in &plans {
            if let Some(volume) = plan.volume() {
                reconcile_project_volume(
                    &mut engine,
                    ProjectVolumeReconcileOptions {
                        request: volume,
                        installation_id: &installation_id,
                        schema_version: 8,
                    },
                )
                .await
                .map_err(|error| SharedInfrastructureReconcileError::Engine {
                    action: "reconcile dedicated cache volume".to_owned(),
                    detail: error.to_string(),
                })?;
            }
            reconcile_project_service(
                &mut engine,
                WorkloadReconcileOptions {
                    request: plan.request(),
                    installation_id: &installation_id,
                    schema_version: 8,
                },
            )
            .await
            .map_err(|error| SharedInfrastructureReconcileError::Engine {
                action: "reconcile dedicated cache service".to_owned(),
                detail: error.to_string(),
            })?;
        }
        for plan in &plans {
            let readiness = plan
                .provisioning_job()
                .expect("dedicated cache readiness job");
            probe_until_ready(&mut engine, readiness, &installation_id).await?;
        }

        Ok::<_, SharedInfrastructureReconcileError>(())
    });
    runtime
        .block_on(delete_owned_installation_resources(
            &mut engine,
            InstallationResourceDeletionOptions {
                installation_id: &installation_id,
                schema_version: 8,
                authorized_persistent_volumes: std::slice::from_ref(&dragonfly_volume),
            },
        ))
        .expect("delete dedicated cache acceptance resources");
    acceptance.expect("run dedicated cache readiness jobs");

    println!("dedicated cache readiness passed for {installation_id}");
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
        .expect("dedicated cache execution service")
}

fn linux_platform() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "linux/amd64",
        "aarch64" => "linux/arm64",
        architecture => panic!("unsupported Engine acceptance architecture '{architecture}'"),
    }
}
