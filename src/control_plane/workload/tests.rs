use super::{
    ApplicationContainerPlan, ApplicationContainerPlanOptions, ApplicationContainerRequestOptions,
    ImmutableProjectApplicationOptions, JavaScriptRuntimeSpec, OrphanedProjectWorkloadOptions,
    ProjectCommand, ProjectCommandPlan, ProjectCommandPlanOptions, ProjectProcessPlan,
    ProjectProcessPlanOptions, ProjectProcessRequestOptions, ProjectRuntimeReconcileOptions,
    RuntimeEnvironment, RuntimeEnvironmentOptions, RuntimeImageBuildPlan,
    RuntimeImageBuildPlanOptions, WorkloadReconcileAction, WorkloadReconcileOptions,
    application_container_request, plan_immutable_project_application, project_process_request,
    reconcile_project_application, reconcile_project_process, reconcile_project_runtime,
    run_project_command, stop_orphaned_project_workloads, workload_resource_record,
};
use crate::control_plane::application::{ProjectSource, plan_project_registry};
use crate::control_plane::engine::{
    CommandExecutionId, CommandExecutor, CommandRequest, CommandSession, CommandStatus,
    ContainerCreateOptions, ContainerDiscovery, ContainerHealth, ContainerId, ContainerLifecycle,
    ContainerLogStream, ContainerRestartPolicy, ContainerState, EngineError, EngineFuture,
    HealthObserver, ImageBuildRequest, ImageBuilder, ImageId, LogChunk, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ObservedContainer, OwnedContainer, ResourceKind,
    RetentionClass, reconstruct_owned_container,
};
use crate::control_plane::state::{
    EnvironmentLifecycle, ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
    ResourceLifecycle, ResourceRecord, ResourceRecordOptions, ResourceRetention,
};
use crate::control_plane::{ProjectIdentity, ServiceIdentity, resolve_execution_plan};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

#[test]
fn equivalent_runtime_inputs_reuse_one_content_addressed_image() {
    let mut first = runtime_image_options();
    first.php_extensions = vec!["redis".to_owned(), "intl".to_owned()];
    first.system_packages = vec!["git".to_owned(), "imagemagick".to_owned()];
    let mut second = runtime_image_options();
    second.php_extensions = vec!["intl".to_owned(), "redis".to_owned()];
    second.system_packages = vec!["imagemagick".to_owned(), "git".to_owned()];

    let first = RuntimeImageBuildPlan::new(first).expect("first runtime image plan");
    let second = RuntimeImageBuildPlan::new(second).expect("second runtime image plan");

    assert_eq!(
        first.compatibility_fingerprint(),
        second.compatibility_fingerprint()
    );
    assert_eq!(
        first.request().input_digest(),
        second.request().input_digest()
    );
    assert_eq!(first.request().output_tag(), second.request().output_tag());
    assert!(first.manifest_json().contains(r#""php_version":"8.4.12""#));
    assert!(
        first
            .manifest_json()
            .contains(r#""composer_version":"2.8.10""#)
    );
    assert!(!first.manifest_json().contains("bill"));
    assert!(
        first
            .request()
            .dockerfile_contents()
            .contains("COPY runtime-installer.sh /opt/stackctl/runtime-installer.sh")
    );
    assert!(
        first
            .request()
            .dockerfile_contents()
            .contains("/opt/stackctl/runtime-installer.sh")
    );
    assert!(first.manifest_json().contains("installer_sha256"));
    assert!(!first.request().dockerfile_contents().contains("curl"));
    assert!(!first.request().dockerfile_contents().contains("http"));
}

#[test]
fn runtime_image_fingerprint_changes_for_every_compatibility_input() {
    let baseline =
        RuntimeImageBuildPlan::new(runtime_image_options()).expect("baseline runtime image plan");
    let mut base = runtime_image_options();
    base.base_image_digest = concat!(
        "ghcr.io/stackctl/runtime-base@sha256:",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    )
    .to_owned();
    let mut platform = runtime_image_options();
    platform.platform = "linux/amd64".to_owned();
    let mut php = runtime_image_options();
    php.php_version = "8.3.23".to_owned();
    let mut extensions = runtime_image_options();
    extensions.php_extensions.push("redis".to_owned());
    let mut packages = runtime_image_options();
    packages.system_packages.push("imagemagick".to_owned());
    let mut composer = runtime_image_options();
    composer.composer_version = "2.8.11".to_owned();
    let mut javascript = runtime_image_options();
    javascript.javascript = Some(JavaScriptRuntimeSpec::Bun {
        version: "1.2.19".to_owned(),
    });
    let mut installer = runtime_image_options();
    installer.installer_revision = "runtime-installer-v2".to_owned();

    for changed in [
        base, platform, php, extensions, packages, composer, javascript, installer,
    ] {
        let changed = RuntimeImageBuildPlan::new(changed).expect("changed runtime image plan");
        assert_ne!(
            baseline.compatibility_fingerprint(),
            changed.compatibility_fingerprint()
        );
        assert_ne!(
            baseline.request().output_tag(),
            changed.request().output_tag()
        );
    }
}

#[test]
fn runtime_image_planning_rejects_ranges_duplicates_and_unsafe_packages() {
    let mut ranged = runtime_image_options();
    ranged.javascript = Some(JavaScriptRuntimeSpec::Node {
        version: ">=22".to_owned(),
    });
    let mut duplicate = runtime_image_options();
    duplicate.php_extensions = vec!["intl".to_owned(), "intl".to_owned()];
    let mut unsafe_package = runtime_image_options();
    unsafe_package.system_packages = vec!["git;curl example.test".to_owned()];

    let ranged = RuntimeImageBuildPlan::new(ranged).expect_err("mutable Node range");
    let duplicate = RuntimeImageBuildPlan::new(duplicate).expect_err("duplicate extension");
    let unsafe_package =
        RuntimeImageBuildPlan::new(unsafe_package).expect_err("unsafe system package");

    assert_eq!(
        ranged.to_string(),
        "JavaScript runtime version '>=22' must be an exact numeric version"
    );
    assert_eq!(
        duplicate.to_string(),
        "runtime image declares PHP extension 'intl' more than once"
    );
    assert_eq!(
        unsafe_package.to_string(),
        "runtime image system package 'git;curl example.test' is invalid"
    );
}

#[test]
fn runtime_image_planning_rejects_an_unverified_installer_artifact() {
    let mut options = runtime_image_options();
    options.installer_sha256 = concat!(
        "sha256:",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    )
    .to_owned();

    let error = RuntimeImageBuildPlan::new(options).expect_err("installer checksum mismatch");

    assert_eq!(
        error.to_string(),
        "runtime installer artifact checksum does not match its expected sha256 digest"
    );
}

#[test]
fn project_runtime_reconciliation_builds_then_starts_the_derived_application() {
    let runtime_image =
        RuntimeImageBuildPlan::new(runtime_image_options()).expect("runtime image plan");
    let metadata = runtime_application_metadata(&runtime_image);
    let mut engine = RecordingWorkloadEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_project_runtime(
            &mut engine,
            project_runtime_options(&runtime_image, metadata, PathBuf::from("/work/bill")),
        ))
        .expect("project runtime reconciliation");

    assert_eq!(
        engine.built.lock().expect("built requests").as_slice(),
        &[runtime_image.request().clone()]
    );
    assert_eq!(engine.created.len(), 1);
    assert_eq!(
        engine.created[0].image(),
        result.runtime_image_id().as_str()
    );
    assert_eq!(result.workload().action(), WorkloadReconcileAction::Created);
    assert_eq!(result.route().domain(), "bill-app.stackctl.localhost");
    assert_eq!(result.route().upstream(), "http://stackctl-bill-app:8080");
}

#[test]
fn project_runtime_reconciliation_validates_before_building() {
    let runtime_image =
        RuntimeImageBuildPlan::new(runtime_image_options()).expect("runtime image plan");
    let metadata = runtime_application_metadata(&runtime_image);
    let mut engine = RecordingWorkloadEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(reconcile_project_runtime(
            &mut engine,
            project_runtime_options(&runtime_image, metadata, PathBuf::from("relative/bill")),
        ))
        .expect_err("relative project source");

    assert_eq!(
        error.to_string(),
        "application source path 'relative/bill' must be absolute"
    );
    assert!(engine.built.lock().expect("built requests").is_empty());
    assert!(engine.created.is_empty());
}

#[test]
fn project_runtime_build_failures_do_not_create_application_containers() {
    let runtime_image =
        RuntimeImageBuildPlan::new(runtime_image_options()).expect("runtime image plan");
    let metadata = runtime_application_metadata(&runtime_image);
    let mut engine = RecordingWorkloadEngine {
        build_failure: Some("offline runtime build failed".to_owned()),
        ..RecordingWorkloadEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(reconcile_project_runtime(
            &mut engine,
            project_runtime_options(&runtime_image, metadata, PathBuf::from("/work/bill")),
        ))
        .expect_err("runtime image build failure");

    assert_eq!(
        error.to_string(),
        "workload build runtime image failed: offline runtime build failed"
    );
    assert!(engine.created.is_empty());
}

#[test]
fn project_tools_and_hooks_are_structured_container_commands() {
    let cases = [
        (
            ProjectCommand::Composer {
                arguments: vec!["install".to_owned(), "--no-interaction".to_owned()],
            },
            vec!["composer", "install", "--no-interaction"],
        ),
        (
            ProjectCommand::Artisan {
                arguments: vec!["migrate".to_owned(), "--force".to_owned()],
            },
            vec!["php", "artisan", "migrate", "--force"],
        ),
        (
            ProjectCommand::NodePackageManager {
                package_manager: super::NodePackageManager::Pnpm,
                arguments: vec!["run".to_owned(), "build".to_owned()],
            },
            vec!["pnpm", "run", "build"],
        ),
        (
            ProjectCommand::Bun {
                arguments: vec!["run".to_owned(), "build".to_owned()],
            },
            vec!["bun", "run", "build"],
        ),
        (
            ProjectCommand::Hook {
                name: "post-create".to_owned(),
                arguments: vec!["php".to_owned(), "artisan".to_owned(), "migrate".to_owned()],
            },
            vec!["php", "artisan", "migrate"],
        ),
        (
            ProjectCommand::Exec {
                arguments: vec!["php".to_owned(), "-v".to_owned()],
            },
            vec!["php", "-v"],
        ),
        (
            ProjectCommand::PhpTool {
                tool: super::PhpTool::PhpStan,
                arguments: vec!["analyse".to_owned(), "--memory-limit=1G".to_owned()],
            },
            vec!["phpstan", "analyse", "--memory-limit=1G"],
        ),
        (
            ProjectCommand::Deno {
                arguments: vec!["task".to_owned(), "check".to_owned()],
            },
            vec!["deno", "task", "check"],
        ),
    ];

    for (command, expected) in cases {
        let plan = ProjectCommandPlan::new(ProjectCommandPlanOptions {
            project: ProjectIdentity::resolve(Some("bill"), Path::new("/work/bill"))
                .expect("project identity"),
            command,
            environment: BTreeMap::from([("APP_ENV".to_owned(), "local".to_owned())]),
            input: Vec::new(),
            timeout: Duration::from_secs(300),
        })
        .expect("project command plan");

        assert_eq!(plan.arguments(), expected);
        assert_eq!(plan.working_directory(), "/workspace");
        assert!(plan.action().contains("project 'bill'"));
        assert!(!plan.arguments().iter().any(|argument| argument == "sh"));
    }
}

#[test]
fn project_hook_validation_and_debug_output_do_not_leak_arguments() {
    let unsafe_command = ProjectCommand::Hook {
        name: "Post Create".to_owned(),
        arguments: vec!["php".to_owned()],
    };
    let sensitive_command = ProjectCommand::Hook {
        name: "post-create".to_owned(),
        arguments: vec!["php".to_owned(), "project-secret".to_owned()],
    };

    let error = ProjectCommandPlan::new(ProjectCommandPlanOptions {
        project: ProjectIdentity::resolve(Some("bill"), Path::new("/work/bill"))
            .expect("project identity"),
        command: unsafe_command,
        environment: BTreeMap::new(),
        input: Vec::new(),
        timeout: Duration::from_secs(300),
    })
    .expect_err("invalid hook name");

    assert_eq!(
        error.to_string(),
        "project hook name 'Post Create' is invalid"
    );
    assert!(!format!("{sensitive_command:?}").contains("project-secret"));
    let sensitive_plan = ProjectCommandPlan::new(ProjectCommandPlanOptions {
        project: ProjectIdentity::resolve(Some("bill"), Path::new("/work/bill"))
            .expect("project identity"),
        command: sensitive_command,
        environment: BTreeMap::new(),
        input: Vec::new(),
        timeout: Duration::from_secs(300),
    })
    .expect("sensitive hook plan");
    assert!(!format!("{sensitive_plan:?}").contains("project-secret"));
}

#[test]
fn project_commands_reject_foreign_application_targets_before_engine_exec() {
    let plan = ProjectCommandPlan::new(ProjectCommandPlanOptions {
        project: ProjectIdentity::resolve(Some("bill"), Path::new("/work/bill"))
            .expect("project identity"),
        command: ProjectCommand::Composer {
            arguments: vec!["install".to_owned()],
        },
        environment: BTreeMap::new(),
        input: Vec::new(),
        timeout: Duration::from_secs(300),
    })
    .expect("project command plan");
    let container = reconstruct_owned_container(
        &ObservedContainer::new(
            ContainerId::new("shop-app"),
            project_application_metadata("shop", "sha256:runtime").labels(),
        ),
        "install-1",
        8,
    )
    .expect("owned foreign application");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(run_project_command(
            &UnreachableCommandExecutor,
            &container,
            &plan,
        ))
        .expect_err("foreign project application");

    assert_eq!(
        error.to_string(),
        "project command for 'bill' cannot execute in project application 'shop'"
    );
}

#[test]
fn project_commands_execute_through_attached_engine_sessions() {
    let plan = ProjectCommandPlan::new(ProjectCommandPlanOptions {
        project: ProjectIdentity::resolve(Some("bill"), Path::new("/work/bill"))
            .expect("project identity"),
        command: ProjectCommand::NodePackageManager {
            package_manager: super::NodePackageManager::Npm,
            arguments: vec!["run".to_owned(), "build".to_owned()],
        },
        environment: BTreeMap::new(),
        input: Vec::new(),
        timeout: Duration::from_secs(300),
    })
    .expect("project command plan");
    let container = reconstruct_owned_container(
        &ObservedContainer::new(
            ContainerId::new("bill-app"),
            project_application_metadata("bill", "sha256:runtime").labels(),
        ),
        "install-1",
        8,
    )
    .expect("owned project application");
    let executor = RecordingProjectCommandExecutor::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("test runtime");

    let output = runtime
        .block_on(run_project_command(&executor, &container, &plan))
        .expect("attached project command");

    assert_eq!(output.stdout(), b"compiled\n");
    assert_eq!(output.stderr(), b"warning\n");

    assert_eq!(
        executor
            .arguments
            .lock()
            .expect("command arguments")
            .as_slice(),
        &[vec!["npm".to_owned(), "run".to_owned(), "build".to_owned()]]
    );
}

#[test]
fn project_applications_use_private_networking_without_host_ports() {
    let plan = application_plan("bill", "/work/bill");

    assert_eq!(plan.container_name(), "stackctl-bill-app");
    assert_eq!(plan.network_name(), "stackctl-private");
    assert_eq!(plan.source_path(), Path::new("/work/bill"));
    assert_eq!(plan.internal_http_port(), 8080);
    assert!(plan.published_ports().is_empty());
    assert_eq!(plan.gateway_route().domain(), "bill-app.stackctl.localhost");
    assert_eq!(
        plan.gateway_route().upstream(),
        "http://stackctl-bill-app:8080"
    );
}

#[test]
fn application_plans_preserve_the_declared_service_identity() {
    let mut options = application_options("bill", "/work/bill");
    options.service = ServiceIdentity::new("reverb").expect("service identity");

    let plan = ApplicationContainerPlan::new(options).expect("reverb application plan");

    assert_eq!(plan.container_name(), "stackctl-bill-reverb");
    assert_eq!(
        plan.gateway_route().domain(),
        "bill-reverb.stackctl.localhost"
    );
    assert_eq!(
        plan.gateway_route().upstream(),
        "http://stackctl-bill-reverb:8080"
    );
}

#[test]
fn equal_runtime_images_still_produce_dedicated_project_containers() {
    let bill = application_plan("bill", "/work/bill");
    let shop = application_plan("shop", "/work/shop");

    assert_eq!(bill.image_digest(), shop.image_digest());
    assert_ne!(bill.container_name(), shop.container_name());
    assert_ne!(bill.source_path(), shop.source_path());
}

#[test]
fn mutable_application_images_fail_before_engine_planning() {
    let mut options = application_options("bill", "/work/bill");
    options.image_digest = "ghcr.io/stackctl/php:8.4".to_owned();

    let error = ApplicationContainerPlan::new(options).expect_err("mutable image");

    assert_eq!(
        error.to_string(),
        "application image 'ghcr.io/stackctl/php:8.4' must use an immutable sha256 digest"
    );
}

#[test]
fn application_plans_accept_derived_engine_content_ids() {
    let mut options = application_options("bill", "/work/bill");
    options.image_digest = format!("sha256:{}", "a".repeat(64));

    let plan = ApplicationContainerPlan::new(options).expect("derived image content ID");

    assert_eq!(plan.image_digest(), format!("sha256:{}", "a".repeat(64)));
}

#[test]
fn relative_application_source_paths_fail_before_engine_mutation() {
    let mut options = application_options("bill", "/work/bill");
    options.source_path = PathBuf::from("relative/bill");

    let error = ApplicationContainerPlan::new(options).expect_err("relative source path");

    assert_eq!(
        error.to_string(),
        "application source path 'relative/bill' must be absolute"
    );
}

#[test]
fn application_plan_materializes_one_private_owned_linux_engine_request() {
    let plan = application_plan("bill", "/work/bill");
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::ProjectApplication,
        project_id: Some("bill".to_owned()),
        compatibility_fingerprint: "sha256:runtime-php-84".to_owned(),
        schema_version: 8,
        desired_revision: "sha256:desired-v1".to_owned(),
        retention: RetentionClass::Disposable,
    })
    .expect("application metadata");

    let request = application_container_request(ApplicationContainerRequestOptions {
        plan,
        metadata,
        platform: "linux/arm64".to_owned(),
        command: vec![
            "stackctl-runtime".to_owned(),
            "serve".to_owned(),
            "--port=8080".to_owned(),
        ],
        environment: runtime_environment(
            "bill",
            BTreeMap::from([("APP_ENV".to_owned(), "local".to_owned())]),
            BTreeMap::from([("DB_PASSWORD".to_owned(), "project-secret".to_owned())]),
        ),
    })
    .expect("application Engine request");

    assert_eq!(request.name(), "stackctl-bill-app");
    assert_eq!(request.platform(), Some("linux/arm64"));
    assert_eq!(request.network(), Some("stackctl-private"));
    assert!(request.port_bindings().is_empty());
    assert_eq!(request.bind_mounts().len(), 1);
    assert_eq!(request.bind_mounts()[0].source(), "/work/bill");
    assert_eq!(request.bind_mounts()[0].target(), "/workspace");
    assert!(!request.bind_mounts()[0].is_read_only());
    assert_eq!(request.command()[0], "stackctl-runtime");
    assert_eq!(
        request.restart_policy(),
        Some(ContainerRestartPolicy::UnlessStopped)
    );
    assert!(request.environment().contains_key("DB_PASSWORD"));
    assert!(!format!("{request:?}").contains("project-secret"));
}

#[test]
fn application_requests_preserve_an_immutable_images_default_command() {
    let plan = application_plan("bill", "/work/bill");
    let metadata = project_application_metadata("bill", "sha256:runtime-php-84");

    let request = application_container_request(ApplicationContainerRequestOptions {
        plan,
        metadata,
        platform: "linux/arm64".to_owned(),
        command: Vec::new(),
        environment: runtime_environment("bill", BTreeMap::new(), BTreeMap::new()),
    })
    .expect("application Engine request using image defaults");

    assert!(request.command().is_empty());
}

#[test]
fn resolved_immutable_applications_produce_exact_engine_and_gateway_plans() {
    let application = resolved_application(concat!(
        "schema_version: 8\nproject: bill\nservices:\n  app:\n",
        "    image: ghcr.io/acme/bill@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"
    ));

    let plan = plan_immutable_project_application(ImmutableProjectApplicationOptions {
        service: &application,
        managed_environment: managed_environment(
            "bill",
            BTreeMap::from([("DATABASE_URL".to_owned(), "postgres://bill".to_owned())]),
            EnvironmentLifecycle::Active,
        ),
        installation_id: "install-1",
        schema_version: 8,
        platform: "linux/arm64",
        network_name: "stackctl",
        internal_http_port: 8080,
    })
    .expect("immutable application plan");

    assert_eq!(plan.request().name(), "stackctl-bill-app");
    assert_eq!(
        plan.request().image(),
        concat!(
            "ghcr.io/acme/bill@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )
    );
    assert_eq!(plan.request().network(), Some("stackctl"));
    assert_eq!(plan.request().metadata().resource_id(), Some("app"));
    assert!(plan.request().command().is_empty());
    assert_eq!(plan.route().domain(), "bill-app.stackctl.localhost");
    assert_eq!(plan.route().upstream(), "http://stackctl-bill-app:8080");
}

#[test]
fn unresolved_or_mutable_application_artifacts_fail_before_engine_mutation() {
    let built_in = resolved_application(
        "schema_version: 8\nproject: bill\nservices:\n  app:\n    preset: laravel\n",
    );
    let mutable = resolved_application(
        "schema_version: 8\nproject: bill\nservices:\n  app:\n    image: ghcr.io/acme/bill:latest\n",
    );

    let built_in_error =
        plan_immutable_project_application(immutable_application_options(&built_in))
            .expect_err("unresolved built-in artifact");
    let mutable_error = plan_immutable_project_application(immutable_application_options(&mutable))
        .expect_err("mutable custom artifact");

    assert_eq!(
        built_in_error.to_string(),
        "project application 'bill-app' requires a resolved immutable image artifact"
    );
    assert_eq!(
        mutable_error.to_string(),
        "application image 'ghcr.io/acme/bill:latest' must use an immutable sha256 digest"
    );
}

#[test]
fn application_rejects_environment_owned_by_another_project() {
    let plan = application_plan("bill", "/work/bill");
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::ProjectApplication,
        project_id: Some("bill".to_owned()),
        compatibility_fingerprint: "sha256:runtime-php-84".to_owned(),
        schema_version: 8,
        desired_revision: "sha256:desired-v1".to_owned(),
        retention: RetentionClass::Disposable,
    })
    .expect("application metadata");

    let error = application_container_request(ApplicationContainerRequestOptions {
        plan,
        metadata,
        platform: "linux/arm64".to_owned(),
        command: vec!["stackctl-runtime".to_owned(), "serve".to_owned()],
        environment: runtime_environment("shop", BTreeMap::new(), BTreeMap::new()),
    })
    .expect_err("foreign runtime environment");

    assert_eq!(
        error.to_string(),
        "application 'bill' cannot use runtime environment owned by 'shop'"
    );
}

#[test]
fn project_application_reconciliation_creates_and_starts_missing_runtime() {
    let request = application_request("sha256:desired-v1");
    let mut engine = RecordingWorkloadEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_project_application(
            &mut engine,
            WorkloadReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("reconcile missing application");

    assert_eq!(result.action(), WorkloadReconcileAction::Created);
    assert_eq!(engine.created, vec![request]);
    assert_eq!(engine.started.len(), 1);
    let resource = workload_resource_record(&result);
    assert_eq!(resource.resource_id(), result.container().id().as_str());
    assert_eq!(resource.scope_id(), Some("app"));
    assert_eq!(resource.project_id(), Some("bill"));
}

#[test]
fn project_application_reconciliation_keeps_healthy_desired_runtime() {
    let request = application_request("sha256:desired-v1");
    let mut engine = RecordingWorkloadEngine {
        observed: vec![ObservedContainer::new(
            ContainerId::new("bill-app"),
            request.metadata().labels(),
        )],
        state: ContainerState::Running,
        health: ContainerHealth::Healthy,
        ..RecordingWorkloadEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_project_application(
            &mut engine,
            WorkloadReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("reconcile healthy application");

    assert_eq!(result.action(), WorkloadReconcileAction::Unchanged);
    assert!(engine.created.is_empty());
    assert!(engine.started.is_empty());
}

#[test]
fn project_application_reconciliation_replaces_disposable_revision_drift() {
    let old_request = application_request("sha256:desired-v1");
    let request = application_request("sha256:desired-v2");
    let mut engine = RecordingWorkloadEngine {
        observed: vec![ObservedContainer::new(
            ContainerId::new("bill-app"),
            old_request.metadata().labels(),
        )],
        state: ContainerState::Running,
        ..RecordingWorkloadEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_project_application(
            &mut engine,
            WorkloadReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("replace drifted application");

    assert_eq!(result.action(), WorkloadReconcileAction::Replaced);
    assert_eq!(engine.stopped.len(), 1);
    assert_eq!(engine.removed.len(), 1);
    assert_eq!(engine.created, vec![request]);
}

#[test]
fn project_application_reconciliation_rejects_duplicate_owned_runtimes() {
    let request = application_request("sha256:desired-v1");
    let labels = request.metadata().labels();
    let mut engine = RecordingWorkloadEngine {
        observed: vec![
            ObservedContainer::new(ContainerId::new("bill-app-1"), labels.clone()),
            ObservedContainer::new(ContainerId::new("bill-app-2"), labels),
        ],
        ..RecordingWorkloadEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(reconcile_project_application(
            &mut engine,
            WorkloadReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect_err("duplicate application runtimes");

    assert_eq!(
        error.to_string(),
        "project 'bill' application 'app' owns 2 containers; refusing to guess"
    );
    assert!(engine.created.is_empty());
}

#[test]
fn project_process_reconciliation_creates_and_starts_a_missing_worker() {
    let request = process_request("queue-worker", "sha256:desired-v1");
    let mut engine = RecordingWorkloadEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_project_process(
            &mut engine,
            WorkloadReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("reconcile missing worker");

    assert_eq!(result.action(), WorkloadReconcileAction::Created);
    assert_eq!(engine.created, vec![request]);
    assert_eq!(engine.started.len(), 1);
}

#[test]
fn orphaned_project_workloads_are_stopped_without_deleting_their_containers() {
    let request = application_request("sha256:desired-v1");
    let observed = ObservedContainer::new(
        ContainerId::new("bill-app-container"),
        request.metadata().labels(),
    );
    let resource = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "bill-app-container".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::ProjectApplication.label().to_owned(),
        compatibility_fingerprint: request.metadata().compatibility_fingerprint().to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: request.metadata().desired_revision().to_owned(),
        retention: ResourceRetention::Disposable,
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(12_345),
    })
    .with_scope_id("app");
    let mut engine = RecordingWorkloadEngine {
        observed: vec![observed],
        state: ContainerState::Running,
        ..RecordingWorkloadEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let stopped = runtime
        .block_on(stop_orphaned_project_workloads(
            &mut engine,
            OrphanedProjectWorkloadOptions {
                resources: &[resource],
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("stop orphaned workloads");

    assert_eq!(stopped, 1);
    assert_eq!(engine.stopped.len(), 1);
    assert_eq!(engine.stopped[0].id().as_str(), "bill-app-container");
    assert!(engine.removed.is_empty());
}

#[test]
fn project_process_reconciliation_selects_only_its_exact_resource_identity() {
    let worker = process_request("queue-worker", "sha256:worker-v1");
    let scheduler = process_request("scheduler", "sha256:scheduler-v1");
    let mut engine = RecordingWorkloadEngine {
        observed: vec![
            ObservedContainer::new(ContainerId::new("bill-worker"), worker.metadata().labels()),
            ObservedContainer::new(
                ContainerId::new("bill-scheduler"),
                scheduler.metadata().labels(),
            ),
        ],
        state: ContainerState::Running,
        health: ContainerHealth::Healthy,
        ..RecordingWorkloadEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_project_process(
            &mut engine,
            WorkloadReconcileOptions {
                request: &worker,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("reconcile exact worker");

    assert_eq!(result.action(), WorkloadReconcileAction::Unchanged);
    assert_eq!(result.container().id().as_str(), "bill-worker");
    assert!(engine.created.is_empty());
}

#[test]
fn project_workers_materialize_as_supervised_private_linux_containers() {
    let project =
        ProjectIdentity::resolve(Some("bill"), Path::new("/work/bill")).expect("project identity");
    let service = ServiceIdentity::new("queue-worker").expect("service identity");
    let plan = ProjectProcessPlan::new(ProjectProcessPlanOptions {
        project,
        service,
        image_digest: concat!(
            "ghcr.io/stackctl/php@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )
        .to_owned(),
        source_path: PathBuf::from("/work/bill"),
        network_name: "stackctl-private".to_owned(),
        command: vec![
            "php".to_owned(),
            "artisan".to_owned(),
            "queue:work".to_owned(),
        ],
        environment: runtime_environment(
            "bill",
            BTreeMap::new(),
            BTreeMap::from([("DB_PASSWORD".to_owned(), "project-secret".to_owned())]),
        ),
    })
    .expect("worker plan");
    assert!(!format!("{plan:?}").contains("project-secret"));
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::ProjectProcess,
        project_id: Some("bill".to_owned()),
        compatibility_fingerprint: "sha256:runtime-php-84".to_owned(),
        schema_version: 8,
        desired_revision: "sha256:desired-v1".to_owned(),
        retention: RetentionClass::Disposable,
    })
    .expect("worker metadata")
    .with_resource_id("queue-worker")
    .expect("worker resource identity");

    let request = project_process_request(ProjectProcessRequestOptions {
        plan,
        metadata,
        platform: "linux/arm64".to_owned(),
    })
    .expect("worker request");

    assert_eq!(request.name(), "stackctl-bill-queue-worker");
    assert_eq!(
        request.metadata().labels()["dev.stackctl.kind"],
        "project_process"
    );
    assert_eq!(
        request.metadata().labels()["dev.stackctl.resource"],
        "queue-worker"
    );
    assert_eq!(request.platform(), Some("linux/arm64"));
    assert_eq!(request.network(), Some("stackctl-private"));
    assert!(request.port_bindings().is_empty());
    assert_eq!(request.bind_mounts()[0].source(), "/work/bill");
    assert_eq!(request.bind_mounts()[0].target(), "/workspace");
    assert_eq!(request.command(), ["php", "artisan", "queue:work"]);
    assert_eq!(
        request.restart_policy(),
        Some(ContainerRestartPolicy::UnlessStopped)
    );
    assert!(!format!("{request:?}").contains("project-secret"));
}

#[test]
fn project_process_rejects_environment_owned_by_another_project() {
    let project =
        ProjectIdentity::resolve(Some("bill"), Path::new("/work/bill")).expect("project identity");
    let service = ServiceIdentity::new("worker").expect("service identity");

    let error = ProjectProcessPlan::new(ProjectProcessPlanOptions {
        project,
        service,
        image_digest: concat!(
            "ghcr.io/stackctl/php@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )
        .to_owned(),
        source_path: PathBuf::from("/work/bill"),
        network_name: "stackctl-private".to_owned(),
        command: vec!["php".to_owned(), "artisan".to_owned()],
        environment: runtime_environment("shop", BTreeMap::new(), BTreeMap::new()),
    })
    .expect_err("foreign runtime environment");

    assert_eq!(
        error.to_string(),
        "project process 'bill:worker' cannot use runtime environment owned by 'shop'"
    );
}

#[test]
fn declared_values_cannot_silently_replace_daemon_managed_environment() {
    let project =
        ProjectIdentity::resolve(Some("bill"), Path::new("/work/bill")).expect("project identity");
    let managed = managed_environment(
        "bill",
        BTreeMap::from([("DB_HOST".to_owned(), "stackctl-postgres-17".to_owned())]),
        EnvironmentLifecycle::Active,
    );

    let error = RuntimeEnvironment::new(RuntimeEnvironmentOptions {
        project,
        declared: BTreeMap::from([("DB_HOST".to_owned(), "localhost".to_owned())]),
        managed,
    })
    .expect_err("managed environment collision");

    assert_eq!(
        error.to_string(),
        "project 'bill' environment key 'DB_HOST' conflicts with its daemon-managed value"
    );
}

fn application_plan(project: &str, path: &str) -> ApplicationContainerPlan {
    ApplicationContainerPlan::new(application_options(project, path))
        .expect("valid application plan")
}

fn application_request(desired_revision: &str) -> ContainerCreateOptions {
    application_container_request(ApplicationContainerRequestOptions {
        plan: application_plan("bill", "/work/bill"),
        metadata: ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
            installation_id: "install-1".to_owned(),
            kind: ResourceKind::ProjectApplication,
            project_id: Some("bill".to_owned()),
            compatibility_fingerprint: "sha256:runtime-php-84".to_owned(),
            schema_version: 8,
            desired_revision: desired_revision.to_owned(),
            retention: RetentionClass::Disposable,
        })
        .expect("application metadata")
        .with_resource_id("app")
        .expect("application resource identity"),
        platform: "linux/arm64".to_owned(),
        command: vec!["stackctl-runtime".to_owned(), "serve".to_owned()],
        environment: runtime_environment("bill", BTreeMap::new(), BTreeMap::new()),
    })
    .expect("application request")
}

fn process_request(service: &str, desired_revision: &str) -> ContainerCreateOptions {
    let project =
        ProjectIdentity::resolve(Some("bill"), Path::new("/work/bill")).expect("project identity");
    let service_identity = ServiceIdentity::new(service).expect("service identity");
    let plan = ProjectProcessPlan::new(ProjectProcessPlanOptions {
        project,
        service: service_identity,
        image_digest: concat!(
            "ghcr.io/stackctl/php@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )
        .to_owned(),
        source_path: PathBuf::from("/work/bill"),
        network_name: "stackctl-private".to_owned(),
        command: vec!["php".to_owned(), "artisan".to_owned(), service.to_owned()],
        environment: runtime_environment("bill", BTreeMap::new(), BTreeMap::new()),
    })
    .expect("process plan");
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::ProjectProcess,
        project_id: Some("bill".to_owned()),
        compatibility_fingerprint: "sha256:runtime-php-84".to_owned(),
        schema_version: 8,
        desired_revision: desired_revision.to_owned(),
        retention: RetentionClass::Disposable,
    })
    .expect("process metadata")
    .with_resource_id(service)
    .expect("process resource identity");

    project_process_request(ProjectProcessRequestOptions {
        plan,
        metadata,
        platform: "linux/arm64".to_owned(),
    })
    .expect("process request")
}

struct RecordingWorkloadEngine {
    built: Mutex<Vec<ImageBuildRequest>>,
    build_failure: Option<String>,
    observed: Vec<ObservedContainer>,
    state: ContainerState,
    health: ContainerHealth,
    created: Vec<ContainerCreateOptions>,
    started: Vec<OwnedContainer>,
    stopped: Vec<OwnedContainer>,
    removed: Vec<OwnedContainer>,
}

impl Default for RecordingWorkloadEngine {
    fn default() -> Self {
        Self {
            built: Mutex::new(Vec::new()),
            build_failure: None,
            observed: Vec::new(),
            state: ContainerState::Missing,
            health: ContainerHealth::Starting,
            created: Vec::new(),
            started: Vec::new(),
            stopped: Vec::new(),
            removed: Vec::new(),
        }
    }
}

impl ImageBuilder for RecordingWorkloadEngine {
    fn build_image<'operation>(
        &'operation self,
        request: &'operation ImageBuildRequest,
    ) -> EngineFuture<'operation, ImageId> {
        self.built
            .lock()
            .expect("built requests")
            .push(request.clone());
        let failure = self.build_failure.clone();
        Box::pin(async move {
            if let Some(detail) = failure {
                return Err(EngineError::Backend { detail });
            }

            ImageId::new(format!("sha256:{}", "b".repeat(64)))
        })
    }
}

struct UnreachableCommandExecutor;

impl CommandExecutor for UnreachableCommandExecutor {
    fn start_command<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
        _request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        panic!("foreign project command reached Engine exec")
    }

    fn command_status<'operation>(
        &'operation self,
        _execution_id: &'operation CommandExecutionId,
        _container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus> {
        panic!("foreign project command reached Engine status")
    }
}

#[derive(Default)]
struct RecordingProjectCommandExecutor {
    arguments: Mutex<Vec<Vec<String>>>,
}

impl CommandExecutor for RecordingProjectCommandExecutor {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        self.arguments
            .lock()
            .expect("command arguments")
            .push(request.arguments().to_vec());
        let container_id = container.id().clone();
        Box::pin(async move {
            let (writer, _reader) = tokio::io::duplex(1024);
            let output: ContainerLogStream<'static> = Box::pin(futures_util::stream::iter([
                Ok(LogChunk::stdout(b"compiled\n".to_vec())),
                Ok(LogChunk::new(
                    crate::control_plane::engine::LogStreamKind::Stderr,
                    b"warning\n".to_vec(),
                )),
            ]));

            Ok(CommandSession::new(
                CommandExecutionId::new("project-command"),
                container_id,
                Box::pin(writer),
                output,
            ))
        })
    }

    fn command_status<'operation>(
        &'operation self,
        _execution_id: &'operation CommandExecutionId,
        _container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus> {
        Box::pin(async { Ok(CommandStatus::Exited(0)) })
    }
}

impl ContainerDiscovery for RecordingWorkloadEngine {
    fn discover_managed(&self) -> EngineFuture<'_, Vec<ObservedContainer>> {
        Box::pin(async { Ok(self.observed.clone()) })
    }
}

impl ContainerLifecycle for RecordingWorkloadEngine {
    fn create<'operation>(
        &'operation mut self,
        options: &'operation ContainerCreateOptions,
    ) -> EngineFuture<'operation, OwnedContainer> {
        Box::pin(async move {
            self.created.push(options.clone());
            reconstruct_owned_container(
                &ObservedContainer::new(
                    ContainerId::new("created-application"),
                    options.metadata().labels(),
                ),
                options.metadata().installation_id(),
                options.metadata().schema_version(),
            )
            .map_err(|ownership| EngineError::Backend {
                detail: format!("could not reconstruct application: {ownership:?}"),
            })
        })
    }

    fn start<'operation>(
        &'operation mut self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.started.push(container.clone());
            Ok(())
        })
    }

    fn stop<'operation>(
        &'operation mut self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.stopped.push(container.clone());
            Ok(())
        })
    }

    fn remove<'operation>(
        &'operation mut self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.removed.push(container.clone());
            Ok(())
        })
    }

    fn inspect<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ContainerState> {
        Box::pin(async { Ok(self.state) })
    }
}

impl HealthObserver for RecordingWorkloadEngine {
    fn observe_health<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ContainerHealth> {
        Box::pin(async { Ok(self.health) })
    }
}

fn application_options(project: &str, path: &str) -> ApplicationContainerPlanOptions {
    ApplicationContainerPlanOptions {
        project: ProjectIdentity::resolve(Some(project), Path::new(path))
            .expect("project identity"),
        service: ServiceIdentity::new("app").expect("service identity"),
        image_digest: concat!(
            "ghcr.io/stackctl/php@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )
        .to_owned(),
        source_path: PathBuf::from(path),
        network_name: "stackctl-private".to_owned(),
        internal_http_port: 8080,
    }
}

fn runtime_image_options() -> RuntimeImageBuildPlanOptions {
    let installer = b"#!/bin/sh\nset -eu\n".to_vec();

    RuntimeImageBuildPlanOptions {
        installation_id: "install-1".to_owned(),
        schema_version: 8,
        base_image_digest: concat!(
            "ghcr.io/stackctl/runtime-base@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )
        .to_owned(),
        platform: "linux/arm64".to_owned(),
        php_version: "8.4.12".to_owned(),
        php_extensions: vec!["intl".to_owned()],
        system_packages: vec!["git".to_owned()],
        composer_version: "2.8.10".to_owned(),
        javascript: Some(JavaScriptRuntimeSpec::Node {
            version: "22.17.0".to_owned(),
        }),
        installer_revision: "runtime-installer-v1".to_owned(),
        installer_sha256: format!("sha256:{}", hex::encode(Sha256::digest(&installer))),
        installer,
    }
}

fn resolved_application(yaml: &str) -> crate::control_plane::ServiceExecutionPlan {
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        yaml.to_owned(),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");

    execution.services()[0].clone()
}

fn immutable_application_options(
    service: &crate::control_plane::ServiceExecutionPlan,
) -> ImmutableProjectApplicationOptions<'_> {
    ImmutableProjectApplicationOptions {
        service,
        managed_environment: managed_environment(
            "bill",
            BTreeMap::new(),
            EnvironmentLifecycle::Active,
        ),
        installation_id: "install-1",
        schema_version: 8,
        platform: "linux/arm64",
        network_name: "stackctl",
        internal_http_port: 8080,
    }
}

fn runtime_application_metadata(runtime_image: &RuntimeImageBuildPlan) -> ManagedResourceMetadata {
    ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::ProjectApplication,
        project_id: Some("bill".to_owned()),
        compatibility_fingerprint: runtime_image.compatibility_fingerprint().to_owned(),
        schema_version: 8,
        desired_revision: "sha256:desired-v1".to_owned(),
        retention: RetentionClass::Disposable,
    })
    .expect("application metadata")
    .with_resource_id("app")
    .expect("application resource identity")
}

fn project_application_metadata(
    project: &str,
    compatibility_fingerprint: &str,
) -> ManagedResourceMetadata {
    ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::ProjectApplication,
        project_id: Some(project.to_owned()),
        compatibility_fingerprint: compatibility_fingerprint.to_owned(),
        schema_version: 8,
        desired_revision: "sha256:desired-v1".to_owned(),
        retention: RetentionClass::Disposable,
    })
    .expect("project application metadata")
    .with_resource_id("app")
    .expect("project application resource identity")
}

fn project_runtime_options<'plan>(
    runtime_image: &'plan RuntimeImageBuildPlan,
    metadata: ManagedResourceMetadata,
    source_path: PathBuf,
) -> ProjectRuntimeReconcileOptions<'plan> {
    ProjectRuntimeReconcileOptions {
        runtime_image,
        project: ProjectIdentity::resolve(Some("bill"), Path::new("/work/bill"))
            .expect("project identity"),
        service: ServiceIdentity::new("app").expect("service identity"),
        source_path,
        network_name: "stackctl-private".to_owned(),
        internal_http_port: 8080,
        application_metadata: metadata,
        platform: "linux/arm64".to_owned(),
        command: vec!["stackctl-runtime".to_owned(), "serve".to_owned()],
        environment: runtime_environment(
            "bill",
            BTreeMap::new(),
            BTreeMap::from([(
                "APP_URL".to_owned(),
                "https://bill-app.stackctl.localhost".to_owned(),
            )]),
        ),
        installation_id: "install-1",
        schema_version: 8,
    }
}

fn runtime_environment(
    project: &str,
    declared: BTreeMap<String, String>,
    managed: BTreeMap<String, String>,
) -> RuntimeEnvironment {
    RuntimeEnvironment::new(RuntimeEnvironmentOptions {
        project: ProjectIdentity::resolve(Some(project), Path::new("/work/bill"))
            .expect("project identity"),
        declared,
        managed: managed_environment(project, managed, EnvironmentLifecycle::Active),
    })
    .expect("runtime environment")
}

fn managed_environment(
    project: &str,
    values: BTreeMap<String, String>,
    lifecycle: EnvironmentLifecycle,
) -> ManagedEnvironmentRecord {
    ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: project.to_owned(),
        revision: "sha256:environment-v1".to_owned(),
        values,
        lifecycle,
    })
}
