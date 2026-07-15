use super::plan_soketi_project_resources;
use crate::control_plane::application::{ProjectSource, plan_project_registry};
use crate::control_plane::engine::{
    BollardEngineAdapter, ContainerHealth, ContainerLifecycle, HealthObserver, ImageResolver,
    ImmutableImageReference, InstallationResourceDeletionOptions, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, NetworkCreateOptions, NetworkManager, ResourceKind,
    RetentionClass, delete_owned_installation_resources,
};
use crate::control_plane::resolve_execution_plan;
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::workload::{
    DedicatedProjectServiceOptions, plan_dedicated_project_service,
};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SOKETI_IMAGE: &str = concat!(
    "quay.io/soketi/soketi@sha256:",
    "087dc4623b043d28f629a63c98d8ae44191f5acbec4ec9e8475fb44193047d03"
);

#[test]
#[ignore = "CI owns live dedicated Soketi readiness acceptance"]
fn live_docker_engine_soketi_becomes_healthy_behind_its_deterministic_route() {
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
                "  realtime:\n    preset: soketi\n    version: '1'\n",
                "    image: {soketi_image}\n"
            ),
            project = project_id,
            soketi_image = SOKETI_IMAGE,
        ),
    );
    let registry = plan_project_registry(&[source]).expect("plan Soketi registry");
    let execution = resolve_execution_plan(&registry).expect("resolve Soketi execution");
    let soketi = execution
        .services()
        .first()
        .expect("Soketi execution service");
    let prepared = plan_soketi_project_resources(
        soketi,
        CredentialSecret::new(format!("soketi-{nonce}-secret")),
    )
    .expect("prepare Soketi");
    let route = prepared.route().expect("Soketi gateway route");
    assert_eq!(
        route.domain(),
        format!("{project_id}-realtime.stackctl.localhost")
    );
    assert_eq!(
        route.upstream(),
        format!("http://stackctl-{project_id}-realtime:6001")
    );
    let platform = linux_platform();
    let plan = plan_dedicated_project_service(DedicatedProjectServiceOptions {
        service: soketi,
        generated_environment: Some(prepared.container_environment()),
        generated_command: prepared.container_command(),
        generated_configuration_mount: prepared.container_configuration_mount(),
        provisioning_job: prepared.provisioning_job(),
        installation_id: &installation_id,
        schema_version: 8,
        platform,
        network_name: &network_name,
    })
    .expect("plan Soketi workload");
    assert!(plan.provisioning_job().is_none());
    let network_metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.clone(),
        kind: ResourceKind::Network,
        project_id: None,
        compatibility_fingerprint: format!("sha256:{}", "a".repeat(64)),
        schema_version: 8,
        desired_revision: format!("sha256:{}", "b".repeat(64)),
        retention: RetentionClass::Persistent,
    })
    .expect("build Soketi network metadata");
    let network =
        NetworkCreateOptions::new(&network_name, network_metadata).expect("build Soketi network");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build Soketi acceptance runtime");
    let mut engine = runtime
        .block_on(BollardEngineAdapter::connect_unix(&socket))
        .expect("negotiate the selected Docker Engine API");

    let acceptance = runtime.block_on(async {
        engine.create_network(&network).await?;
        let image = ImmutableImageReference::new(plan.request().image())?;
        engine.ensure_image(&image).await?;
        let container = engine.create(plan.request()).await?;
        engine.start(&container).await?;
        wait_until_healthy(&engine, &container).await
    });
    runtime
        .block_on(delete_owned_installation_resources(
            &mut engine,
            InstallationResourceDeletionOptions {
                installation_id: &installation_id,
                schema_version: 8,
                authorized_persistent_volumes: &[],
            },
        ))
        .expect("delete Soketi acceptance resources");
    acceptance.expect("run Soketi health acceptance");

    println!("Soketi readiness passed for {installation_id}");
}

async fn wait_until_healthy(
    engine: &BollardEngineAdapter,
    container: &crate::control_plane::engine::OwnedContainer,
) -> Result<(), crate::control_plane::engine::EngineError> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let health = engine.observe_health(container).await?;
        match health {
            ContainerHealth::Healthy => return Ok(()),
            ContainerHealth::Starting | ContainerHealth::RunningUnverified
                if tokio::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            ContainerHealth::Missing
            | ContainerHealth::Stopped
            | ContainerHealth::Restarting
            | ContainerHealth::Starting
            | ContainerHealth::RunningUnverified
            | ContainerHealth::Unhealthy { .. } => {
                return Err(crate::control_plane::engine::EngineError::Backend {
                    detail: format!("Soketi did not become healthy: {health:?}"),
                });
            }
        }
    }
}

fn linux_platform() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "linux/amd64",
        "aarch64" => "linux/arm64",
        architecture => panic!("unsupported Engine acceptance architecture '{architecture}'"),
    }
}
