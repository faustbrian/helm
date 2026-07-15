use super::{
    BollardEngineAdapter, ContainerCreateOptions, ContainerLifecycle, EngineError, ImageBuilder,
    ImageResolver, InstallationResourceDeletionOptions, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ResourceKind, RetentionClass,
    delete_owned_installation_resources,
};
use crate::control_plane::ProjectIdentity;
use crate::control_plane::workload::{
    NodePackageManager, ProjectCommand, ProjectCommandPlan, ProjectCommandPlanOptions,
    RuntimeImageBuildOptions, RuntimeImageBuildPlan, run_project_command,
};
use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const PHP_IMAGE: &str = concat!(
    "dunglas/frankenphp@sha256:",
    "99a8142b702d9387682b3c845ae21f41ac611961aa20237357e73c93acee6ad2"
);
const COMPOSER_IMAGE: &str = concat!(
    "composer@sha256:",
    "5946476338742b200bb9ff88f8be56275ddae4b3949c72305cb0dbf10cfcb760"
);
const NODE_IMAGE: &str = concat!(
    "node@sha256:",
    "6c74791e557ce11fc957704f6d4fe134a7bc8d6f5ca4403205b2966bd488f6b3"
);
const BUN_IMAGE: &str = concat!(
    "oven/bun@sha256:",
    "e10577f0db68676a7024391c6e5cb4b879ebd17188ab750cf10024a6d700e5c4"
);

#[test]
#[ignore = "CI owns live Linux application runtime and tool acceptance"]
fn live_docker_engine_application_runtime_executes_declared_tools_and_hook() {
    let socket = std::env::var_os("STACKCTL_ENGINE_SOCKET")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/var/run/docker.sock"));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must follow the Unix epoch")
        .as_nanos();
    let installation_id = format!("ci-{}-{nonce}", std::process::id());
    let container_name = format!("stackctl-{installation_id}-app");
    let platform = linux_platform();
    let runtime_plan = RuntimeImageBuildPlan::for_application_runtime(RuntimeImageBuildOptions {
        installation_id: &installation_id,
        schema_version: 8,
        base_image_digest: PHP_IMAGE,
        platform,
        php_extensions: Vec::new(),
        composer_image: Some(COMPOSER_IMAGE),
        node_image: Some(NODE_IMAGE),
        bun_image: Some(BUN_IMAGE),
    })
    .expect("plan immutable application runtime");
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.clone(),
        kind: ResourceKind::ProjectApplication,
        project_id: Some("ci-project".to_owned()),
        compatibility_fingerprint: runtime_plan.compatibility_fingerprint().to_owned(),
        schema_version: 8,
        desired_revision: format!("sha256:{}", "d".repeat(64)),
        retention: RetentionClass::Disposable,
    })
    .and_then(|metadata| metadata.with_resource_id("app"))
    .expect("build application runtime metadata");
    let project = ProjectIdentity::resolve(Some("ci-project"), std::path::Path::new("ci-project"))
        .expect("build application runtime project identity");
    let commands = [
        command_plan(
            &project,
            ProjectCommand::Exec {
                arguments: vec![
                    "php".to_owned(),
                    "-r".to_owned(),
                    "echo PHP_OS_FAMILY . ':' . getcwd();".to_owned(),
                ],
            },
            BTreeMap::new(),
        ),
        command_plan(
            &project,
            ProjectCommand::Composer {
                arguments: vec!["--version".to_owned(), "--no-ansi".to_owned()],
            },
            BTreeMap::new(),
        ),
        command_plan(
            &project,
            ProjectCommand::NodePackageManager {
                package_manager: NodePackageManager::Npm,
                arguments: vec!["--version".to_owned()],
            },
            BTreeMap::new(),
        ),
        command_plan(
            &project,
            ProjectCommand::Bun {
                arguments: vec!["--version".to_owned()],
            },
            BTreeMap::new(),
        ),
        command_plan(
            &project,
            ProjectCommand::Hook {
                name: "post-install".to_owned(),
                arguments: vec![
                    "php".to_owned(),
                    "-r".to_owned(),
                    "echo getenv('STACKCTL_ACCEPTANCE');".to_owned(),
                ],
            },
            BTreeMap::from([("STACKCTL_ACCEPTANCE".to_owned(), "hook".to_owned())]),
        ),
    ];
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build application runtime acceptance runtime");
    let mut engine = runtime
        .block_on(BollardEngineAdapter::connect_unix(&socket))
        .expect("negotiate the selected Docker Engine API");

    let acceptance = runtime.block_on(async {
        for image in runtime_plan.input_images() {
            engine.ensure_image(image).await?;
        }
        let runtime_image = engine.build_image(runtime_plan.request()).await?;
        let request =
            ContainerCreateOptions::new(&container_name, runtime_image.as_str(), metadata)?
                .with_platform(platform)?
                .with_command(vec![
                    "sh".to_owned(),
                    "-c".to_owned(),
                    "mkdir -p /workspace && exec sleep 300".to_owned(),
                ])?;
        let container = engine.create(&request).await?;
        engine.start(&container).await?;

        let mut outputs = Vec::with_capacity(commands.len());
        for command in &commands {
            outputs.push(run_project_command(&engine, &container, command).await?);
        }

        Ok::<_, EngineError>(outputs)
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
        .expect("delete application runtime acceptance resources");
    let outputs = acceptance.expect("execute all declared application runtime tools");

    assert_eq!(outputs.len(), 5);
    assert_eq!(outputs[0].stdout(), b"Linux:/workspace");
    assert!(
        String::from_utf8_lossy(outputs[1].stdout()).starts_with("Composer version "),
        "unexpected Composer output: {}",
        String::from_utf8_lossy(outputs[1].stdout())
    );
    assert!(!outputs[2].stdout().is_empty());
    assert!(!outputs[3].stdout().is_empty());
    assert_eq!(outputs[4].stdout(), b"hook");
    assert!(
        String::from_utf8_lossy(outputs[1].stderr()).starts_with("PHP version "),
        "unexpected Composer diagnostic: {}",
        String::from_utf8_lossy(outputs[1].stderr())
    );
    for output in [&outputs[0], &outputs[2], &outputs[3], &outputs[4]] {
        assert!(
            output.stderr().is_empty(),
            "application tool emitted stderr: {}",
            String::from_utf8_lossy(output.stderr())
        );
    }

    println!("application runtime acceptance passed for {installation_id}");
}

fn command_plan(
    project: &ProjectIdentity,
    command: ProjectCommand,
    environment: BTreeMap<String, String>,
) -> ProjectCommandPlan {
    ProjectCommandPlan::new(ProjectCommandPlanOptions {
        project: project.clone(),
        command,
        environment,
        input: Vec::new(),
        timeout: Duration::from_secs(30),
        browser_session: false,
    })
    .expect("plan application runtime command")
}

fn linux_platform() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "linux/amd64",
        "aarch64" => "linux/arm64",
        architecture => panic!("unsupported Engine acceptance architecture '{architecture}'"),
    }
}
