use super::{
    ApplicationContainerPlan, ApplicationContainerPlanOptions, ApplicationContainerRequestOptions,
    BuildImageGarbageCollectionOptions, DisposableContainerGarbageCollectionOptions,
    EphemeralBrowserOptions, ImmutableProjectApplicationOptions, OrphanedProjectWorkloadOptions,
    ProjectCommand, ProjectCommandPlan, ProjectCommandPlanOptions, ProjectProcessPlan,
    ProjectProcessPlanOptions, ProjectProcessRequestOptions, ProjectVolumeReconcileAction,
    ProjectVolumeReconcileOptions, RuntimeEnvironment, RuntimeEnvironmentOptions,
    WorkloadReconcileAction, WorkloadReconcileError, WorkloadReconcileOptions,
    application_container_request, garbage_collect_build_images,
    garbage_collect_disposable_containers, garbage_collect_disposable_containers_from_observed,
    materialize_application_request, materialize_application_requests, plan_ephemeral_browser,
    plan_immutable_project_application, project_process_request, reconcile_project_application,
    reconcile_project_application_from_observed, reconcile_project_process,
    reconcile_project_service, reconcile_project_service_from_observed, reconcile_project_volume,
    reconcile_project_volume_from_observed, remove_stale_ephemeral_services, run_project_command,
    stop_orphaned_project_workloads, stop_orphaned_project_workloads_from_observed,
    workload_resource_record,
};
use crate::control_plane::application::{ProjectSource, plan_project_registry};
use crate::control_plane::engine::{
    CommandExecutionId, CommandExecutor, CommandRequest, CommandSession, CommandStatus,
    ContainerCreateOptions, ContainerDiscovery, ContainerHealth, ContainerId, ContainerLifecycle,
    ContainerLogStream, ContainerRestartPolicy, ContainerState, EngineError, EngineFuture,
    HealthObserver, ImageBuildRequest, ImageBuilder, ImageDiscovery, ImageId, ImageManager,
    ImageResolver, ImmutableImageReference, LogChunk, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ObservedContainer, ObservedImage, ObservedVolume,
    OwnedContainer, OwnedImage, OwnedVolume, ResourceKind, RetentionClass, VolumeCreateOptions,
    VolumeDiscovery, VolumeManager, reconstruct_owned_container,
};
use crate::control_plane::state::{
    EnvironmentLifecycle, ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
    ResourceLifecycle, ResourceRecord, ResourceRecordOptions, ResourceRetention,
};
use crate::control_plane::{ProjectIdentity, ServiceIdentity, resolve_execution_plan};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

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
            browser_session: false,
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
        browser_session: false,
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
        browser_session: false,
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
        browser_session: false,
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
        browser_session: false,
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
fn declared_php_extensions_produce_a_content_addressed_application_runtime() {
    let application = resolved_application(concat!(
        "schema_version: 8\nproject: bill\nservices:\n  app:\n",
        "    preset: laravel\n    version: \"8.5\"\n",
        "    image: dunglas/frankenphp@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
        "    php_extensions: [redis, intl]\n"
    ));

    let plan = plan_immutable_project_application(ImmutableProjectApplicationOptions {
        service: &application,
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
    })
    .expect("extension-aware application plan");

    let runtime = plan
        .runtime_image()
        .expect("declared extensions require a derived runtime image");
    assert!(
        runtime
            .request()
            .dockerfile_contents()
            .contains("RUN [\"docker-php-ext-enable\",\"intl\",\"redis\"]")
    );
    assert!(runtime.request().dockerfile_contents().contains(concat!(
        "RUN [\"php\",\"-r\",",
        "\"foreach (array_slice($argv, 1) as $extension) { ",
        "if (!extension_loaded($extension)) { ",
        "fwrite(STDERR, 'missing PHP extension: ' . $extension . PHP_EOL); ",
        "exit(1); } }\",\"intl\",\"redis\"]"
    )));
    assert!(
        !runtime
            .request()
            .dockerfile_contents()
            .contains("install-php-extensions")
    );
    assert_eq!(runtime.request().metadata().installation_id(), "install-1");
    assert_eq!(
        runtime.request().metadata().retention(),
        RetentionClass::BuildCache
    );
}

#[test]
fn declared_tool_images_produce_one_content_addressed_application_runtime() {
    let application = resolved_application(concat!(
        "schema_version: 8\nproject: bill\nservices:\n  app:\n",
        "    preset: laravel\n    version: \"8.5\"\n",
        "    image: dunglas/frankenphp@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
        "    composer_image: composer@sha256:",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n",
        "    node_image: node@sha256:",
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\n",
        "    bun_image: oven/bun@sha256:",
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd\n"
    ));

    let plan = plan_immutable_project_application(ImmutableProjectApplicationOptions {
        service: &application,
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
    })
    .expect("tool-aware application plan");

    let dockerfile = plan
        .runtime_image()
        .expect("declared tools require a derived runtime image")
        .request()
        .dockerfile_contents();
    assert!(dockerfile.contains(concat!(
        "FROM composer@sha256:",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb AS stackctl_composer"
    )));
    assert!(dockerfile.contains(concat!(
        "FROM node@sha256:",
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc AS stackctl_node"
    )));
    assert!(dockerfile.contains(concat!(
        "FROM oven/bun@sha256:",
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd AS stackctl_bun"
    )));
    assert!(
        dockerfile
            .contains("COPY --from=stackctl_composer /usr/bin/composer /usr/local/bin/composer")
    );
    assert!(dockerfile.contains("COPY --from=stackctl_node /usr/local/ /usr/local/"));
    assert!(dockerfile.contains("COPY --from=stackctl_bun /usr/local/bin/bun /usr/local/bin/bun"));

    let mut engine = RecordingWorkloadEngine::default();
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime")
        .block_on(materialize_application_request(&mut engine, &plan))
        .expect("materialized tool runtime");
    assert_eq!(
        *engine.resolved_images.lock().expect("resolved images"),
        [
            concat!(
                "dunglas/frankenphp@sha256:",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            ),
            concat!(
                "composer@sha256:",
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            ),
            concat!(
                "node@sha256:",
                "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
            ),
            concat!(
                "oven/bun@sha256:",
                "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
            ),
        ]
    );
}

#[test]
fn equal_application_runtimes_are_materialized_once_per_pass() {
    let plan = |project: &str| {
        let application = resolved_application(&format!(
            concat!(
                "schema_version: 8\nproject: {}\nservices:\n  app:\n",
                "    preset: laravel\n    version: \"8.5\"\n",
                "    image: dunglas/frankenphp@sha256:",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
                "    php_extensions: [redis, intl]\n"
            ),
            project
        ));

        plan_immutable_project_application(ImmutableProjectApplicationOptions {
            service: &application,
            managed_environment: managed_environment(
                project,
                BTreeMap::new(),
                EnvironmentLifecycle::Active,
            ),
            installation_id: "install-1",
            schema_version: 8,
            platform: "linux/arm64",
            network_name: "stackctl",
            internal_http_port: 8080,
        })
        .expect("runtime application plan")
    };
    let applications = [plan("bill"), plan("shop")];
    let mut engine = RecordingWorkloadEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let requests = runtime
        .block_on(materialize_application_requests(&mut engine, &applications))
        .expect("materialized application requests");

    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].name(), "stackctl-bill-app");
    assert_eq!(requests[1].name(), "stackctl-shop-app");
    assert_eq!(requests[0].image(), requests[1].image());
    assert_eq!(engine.built.lock().expect("built requests").len(), 1);
    assert_eq!(
        engine
            .resolved_images
            .lock()
            .expect("resolved images")
            .len(),
        1
    );
}

#[test]
fn extension_aware_application_requests_use_the_built_image_identity() {
    let application = resolved_application(concat!(
        "schema_version: 8\nproject: bill\nservices:\n  app:\n",
        "    preset: laravel\n    version: \"8.5\"\n",
        "    image: dunglas/frankenphp@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
        "    php_extensions: [intl]\n"
    ));
    let plan = plan_immutable_project_application(ImmutableProjectApplicationOptions {
        service: &application,
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
    })
    .expect("extension-aware application plan");
    let mut engine = RecordingWorkloadEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let request = runtime
        .block_on(materialize_application_request(&mut engine, &plan))
        .expect("materialized application request");

    assert_eq!(request.image(), format!("sha256:{}", "b".repeat(64)));
    assert_eq!(engine.built.lock().expect("built images").len(), 1);
}

#[test]
fn ephemeral_browser_plans_are_private_disposable_and_operation_scoped() {
    let browser = resolved_application(concat!(
        "schema_version: 8\nproject: bill\nservices:\n  browser:\n",
        "    preset: dusk\n    version: \"4\"\n",
        "    image: selenium/standalone-chromium@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"
    ));
    let options = EphemeralBrowserOptions {
        service: &browser,
        operation_id: "operation-42",
        installation_id: "install-1",
        schema_version: 8,
        platform: "linux/arm64",
        network_name: "stackctl",
    };

    let plan = plan_ephemeral_browser(options).expect("ephemeral browser plan");
    let repeated = plan_ephemeral_browser(EphemeralBrowserOptions {
        service: &browser,
        operation_id: "operation-42",
        installation_id: "install-1",
        schema_version: 8,
        platform: "linux/arm64",
        network_name: "stackctl",
    })
    .expect("repeat browser plan");

    assert_eq!(plan, repeated);
    assert!(plan.request().name().starts_with("stackctl-ephemeral-"));
    assert_eq!(
        plan.request().image(),
        concat!(
            "selenium/standalone-chromium@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )
    );
    assert_eq!(
        plan.request().metadata().kind(),
        ResourceKind::EphemeralService
    );
    assert_eq!(plan.request().metadata().project_id(), Some("bill"));
    assert_eq!(plan.request().metadata().resource_id(), Some("browser"));
    assert_eq!(plan.request().network(), Some("stackctl"));
    assert_eq!(plan.request().platform(), Some("linux/arm64"));
    assert_eq!(plan.request().shared_memory_bytes(), Some(2_147_483_648));
    assert!(plan.request().port_bindings().is_empty());
    assert_eq!(plan.request().restart_policy(), None);
    assert_eq!(
        plan.request()
            .health_check()
            .expect("Selenium readiness")
            .engine_test(),
        [
            "CMD",
            "/opt/bin/check-grid.sh",
            "--host",
            "0.0.0.0",
            "--port",
            "4444"
        ]
    );
    assert_eq!(
        plan.command_environment().get("DUSK_DRIVER_URL"),
        Some(&format!("http://{}:4444/wd/hub", plan.request().name()))
    );
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
fn project_application_reconciliation_reuses_a_pass_wide_observation() {
    let request = application_request("sha256:desired-v1");
    let observed = [ObservedContainer::new(
        ContainerId::new("bill-app"),
        request.metadata().labels(),
    )];
    let mut engine = RecordingWorkloadEngine {
        state: ContainerState::Running,
        health: ContainerHealth::Healthy,
        ..RecordingWorkloadEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_project_application_from_observed(
            &mut engine,
            &observed,
            WorkloadReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("reconcile from shared observation");

    assert_eq!(result.action(), WorkloadReconcileAction::Unchanged);
    assert!(engine.created.is_empty());
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
fn dedicated_project_service_reconciliation_creates_exact_missing_service() {
    let request = dedicated_service_request("cache", "sha256:desired-v1");
    let mut engine = RecordingWorkloadEngine::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_project_service(
            &mut engine,
            WorkloadReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("reconcile missing dedicated service");

    assert_eq!(result.action(), WorkloadReconcileAction::Created);
    assert_eq!(
        result.container().metadata().kind(),
        ResourceKind::ProjectService
    );
    assert_eq!(result.container().metadata().resource_id(), Some("cache"));
    assert_eq!(engine.created, vec![request]);
    assert_eq!(engine.started.len(), 1);
}

#[test]
fn dedicated_project_service_reconciliation_reuses_a_pass_wide_observation() {
    let request = dedicated_service_request("cache", "sha256:desired-v1");
    let observed = [ObservedContainer::new(
        ContainerId::new("bill-cache"),
        request.metadata().labels(),
    )];
    let mut engine = RecordingWorkloadEngine {
        state: ContainerState::Running,
        health: ContainerHealth::Healthy,
        ..RecordingWorkloadEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_project_service_from_observed(
            &mut engine,
            &observed,
            WorkloadReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("reconcile dedicated service from shared observation");

    assert_eq!(result.action(), WorkloadReconcileAction::Unchanged);
    assert!(engine.created.is_empty());
}

#[test]
fn retained_project_volume_adopts_only_its_exact_data_identity() {
    let request = project_volume_request("sha256:localstack-4");
    let mut engine = RecordingProjectVolumeEngine {
        observed: vec![ObservedVolume::new(
            request.name(),
            request.metadata().labels(),
        )],
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_project_volume(
            &mut engine,
            ProjectVolumeReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("adopt exact project volume");

    assert_eq!(result.action(), ProjectVolumeReconcileAction::Unchanged);
    assert_eq!(result.volume().name(), request.name());
}

#[test]
fn retained_project_volume_reconciliation_reuses_a_pass_wide_observation() {
    let request = project_volume_request("sha256:localstack-4");
    let observed = [ObservedVolume::new(
        request.name(),
        request.metadata().labels(),
    )];
    let mut engine = RecordingProjectVolumeEngine {
        observed: Vec::new(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_project_volume_from_observed(
            &mut engine,
            &observed,
            ProjectVolumeReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("reconcile retained volume from shared observation");

    assert_eq!(result.action(), ProjectVolumeReconcileAction::Unchanged);
    assert_eq!(result.volume().name(), request.name());
}

#[test]
fn retained_project_volume_requires_migration_for_identity_drift() {
    let old = project_volume_request("sha256:localstack-3");
    let request = project_volume_request("sha256:localstack-4");
    let mut engine = RecordingProjectVolumeEngine {
        observed: vec![ObservedVolume::new(old.name(), old.metadata().labels())],
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(reconcile_project_volume(
            &mut engine,
            ProjectVolumeReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect_err("volume identity drift");

    assert!(error.to_string().contains("explicit migration is required"));
    assert!(matches!(
        error,
        WorkloadReconcileError::DestructiveReplacementRequired { .. }
    ));
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
fn orphaned_project_workload_cleanup_reuses_a_pass_wide_observation() {
    let request = application_request("sha256:desired-v1");
    let observed = [ObservedContainer::new(
        ContainerId::new("bill-app-container"),
        request.metadata().labels(),
    )];
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
        state: ContainerState::Running,
        ..RecordingWorkloadEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let stopped = runtime
        .block_on(stop_orphaned_project_workloads_from_observed(
            &mut engine,
            &observed,
            OrphanedProjectWorkloadOptions {
                resources: &[resource],
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("stop orphaned workloads from shared observation");

    assert_eq!(stopped, 1);
    assert_eq!(engine.stopped.len(), 1);
    assert!(engine.removed.is_empty());
}

#[test]
fn interrupted_ephemeral_services_are_stopped_and_removed_on_reconciliation() {
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::EphemeralService,
        project_id: Some("bill".to_owned()),
        compatibility_fingerprint: "sha256:browser".to_owned(),
        schema_version: 8,
        desired_revision: "sha256:operation-42".to_owned(),
        retention: RetentionClass::Disposable,
    })
    .expect("browser metadata")
    .with_resource_id("browser")
    .expect("browser identity");
    let mut engine = RecordingWorkloadEngine {
        observed: vec![ObservedContainer::new(
            ContainerId::new("interrupted-browser"),
            metadata.labels(),
        )],
        state: ContainerState::Running,
        ..RecordingWorkloadEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let removed = runtime
        .block_on(remove_stale_ephemeral_services(&mut engine, "install-1", 8))
        .expect("remove interrupted browser");

    assert_eq!(removed, 1);
    assert_eq!(engine.stopped[0].id().as_str(), "interrupted-browser");
    assert_eq!(engine.removed[0].id().as_str(), "interrupted-browser");
}

#[test]
fn garbage_collection_removes_only_expired_exact_owned_disposable_orphans() {
    let expired_metadata = project_application_metadata("bill", "sha256:runtime-v1");
    let unexpired_metadata = project_application_metadata("shop", "sha256:runtime-v1");
    let persistent_metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::ProjectService,
        project_id: Some("bill".to_owned()),
        compatibility_fingerprint: "sha256:database-v1".to_owned(),
        schema_version: 8,
        desired_revision: "sha256:desired-v1".to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("persistent metadata")
    .with_resource_id("database")
    .expect("persistent identity");
    let expired = orphaned_container_resource("expired-app", &expired_metadata, 100);
    let unexpired = orphaned_container_resource("unexpired-app", &unexpired_metadata, 900);
    let persistent = orphaned_container_resource("persistent-database", &persistent_metadata, 100);
    let mut engine = RecordingWorkloadEngine {
        observed: vec![
            ObservedContainer::new(ContainerId::new("expired-app"), expired_metadata.labels()),
            ObservedContainer::new(
                ContainerId::new("unexpired-app"),
                unexpired_metadata.labels(),
            ),
            ObservedContainer::new(
                ContainerId::new("persistent-database"),
                persistent_metadata.labels(),
            ),
        ],
        state: ContainerState::Running,
        ..RecordingWorkloadEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let retired = runtime
        .block_on(garbage_collect_disposable_containers(
            &mut engine,
            DisposableContainerGarbageCollectionOptions {
                resources: &[expired.clone(), unexpired, persistent],
                installation_id: "install-1",
                schema_version: 8,
                now_unix_seconds: 1_000,
                orphan_retention_seconds: 500,
            },
        ))
        .expect("collect expired disposable container");

    assert_eq!(retired, vec![expired]);
    assert_eq!(engine.stopped.len(), 1);
    assert_eq!(engine.stopped[0].id().as_str(), "expired-app");
    assert_eq!(engine.removed.len(), 1);
    assert_eq!(engine.removed[0].id().as_str(), "expired-app");
}

#[test]
fn disposable_garbage_collection_reuses_a_pass_wide_observation() {
    let metadata = project_application_metadata("bill", "sha256:runtime-v1");
    let expired = orphaned_container_resource("expired-app", &metadata, 100);
    let observed = [ObservedContainer::new(
        ContainerId::new("expired-app"),
        metadata.labels(),
    )];
    let mut engine = RecordingWorkloadEngine {
        state: ContainerState::Running,
        ..RecordingWorkloadEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let retired = runtime
        .block_on(garbage_collect_disposable_containers_from_observed(
            &mut engine,
            &observed,
            DisposableContainerGarbageCollectionOptions {
                resources: std::slice::from_ref(&expired),
                installation_id: "install-1",
                schema_version: 8,
                now_unix_seconds: 1_000,
                orphan_retention_seconds: 500,
            },
        ))
        .expect("collect from shared observation");

    assert_eq!(retired, vec![expired]);
    assert_eq!(engine.stopped.len(), 1);
    assert_eq!(engine.removed.len(), 1);
}

#[test]
fn garbage_collection_refuses_durable_and_engine_ownership_drift() {
    let metadata = project_application_metadata("bill", "sha256:runtime-v1");
    let mut drifted = orphaned_container_resource("expired-app", &metadata, 100);
    drifted = ResourceRecord::new(ResourceRecordOptions {
        resource_id: drifted.resource_id().to_owned(),
        installation_id: drifted.installation_id().to_owned(),
        kind: drifted.kind().to_owned(),
        compatibility_fingerprint: "sha256:other-runtime".to_owned(),
        project_id: drifted.project_id().map(str::to_owned),
        schema_version: drifted.schema_version(),
        desired_revision: drifted.desired_revision().to_owned(),
        retention: drifted.retention(),
        lifecycle: drifted.lifecycle(),
        orphaned_at_unix_seconds: drifted.orphaned_at_unix_seconds(),
    })
    .with_scope_id("app");
    let mut engine = RecordingWorkloadEngine {
        observed: vec![ObservedContainer::new(
            ContainerId::new("expired-app"),
            metadata.labels(),
        )],
        state: ContainerState::Stopped,
        ..RecordingWorkloadEngine::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(garbage_collect_disposable_containers(
            &mut engine,
            DisposableContainerGarbageCollectionOptions {
                resources: &[drifted],
                installation_id: "install-1",
                schema_version: 8,
                now_unix_seconds: 1_000,
                orphan_retention_seconds: 500,
            },
        ))
        .expect_err("ownership drift must block garbage collection");

    assert!(
        error
            .to_string()
            .contains("differs from its durable ownership")
    );
    assert!(engine.stopped.is_empty());
    assert!(engine.removed.is_empty());
}

#[test]
fn build_image_garbage_collection_removes_only_expired_unreferenced_inactive_images() {
    let expired = observed_build_image('a', 100, 0, build_image_metadata().labels());
    let active = observed_build_image('b', 100, 0, build_image_metadata().labels());
    let fresh = observed_build_image('c', 900, 0, build_image_metadata().labels());
    let referenced = observed_build_image('d', 100, 1, build_image_metadata().labels());
    let unknown_references = observed_build_image('e', 100, -1, build_image_metadata().labels());
    let active_id = active.id().as_str().to_owned();
    let mut engine = RecordingImageGarbageCollectionEngine {
        observed: vec![
            expired.clone(),
            active,
            fresh,
            referenced,
            unknown_references,
        ],
        removed: Vec::new(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let removed = runtime
        .block_on(garbage_collect_build_images(
            &mut engine,
            BuildImageGarbageCollectionOptions {
                active_image_ids: &[active_id],
                installation_id: "install-1",
                schema_version: 8,
                now_unix_seconds: 1_000,
                retention_seconds: 500,
            },
        ))
        .expect("collect expired build image");

    assert_eq!(removed, vec![expired.id().clone()]);
    assert_eq!(engine.removed, vec![expired.id().clone()]);
}

#[test]
fn build_image_garbage_collection_validates_all_owned_labels_before_mutation() {
    let valid = observed_build_image('a', 100, 0, build_image_metadata().labels());
    let malformed = observed_build_image(
        'b',
        100,
        0,
        project_application_metadata("bill", "sha256:runtime-v1").labels(),
    );
    let mut engine = RecordingImageGarbageCollectionEngine {
        observed: vec![valid, malformed],
        removed: Vec::new(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(garbage_collect_build_images(
            &mut engine,
            BuildImageGarbageCollectionOptions {
                active_image_ids: &[],
                installation_id: "install-1",
                schema_version: 8,
                now_unix_seconds: 1_000,
                retention_seconds: 500,
            },
        ))
        .expect_err("incorrectly classified image must fail closed");

    assert!(error.to_string().contains("not a disposable build cache"));
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

fn dedicated_service_request(service: &str, desired_revision: &str) -> ContainerCreateOptions {
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::ProjectService,
        project_id: Some("bill".to_owned()),
        compatibility_fingerprint: "sha256:memcached-1".to_owned(),
        schema_version: 8,
        desired_revision: desired_revision.to_owned(),
        retention: RetentionClass::Disposable,
    })
    .expect("dedicated service metadata")
    .with_resource_id(service)
    .expect("dedicated service resource identity");

    ContainerCreateOptions::new(
        format!("stackctl-bill-{service}"),
        concat!(
            "memcached@sha256:",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        ),
        metadata,
    )
    .expect("dedicated service request")
}

fn project_volume_request(compatibility_fingerprint: &str) -> VolumeCreateOptions {
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::Volume,
        project_id: Some("bill".to_owned()),
        compatibility_fingerprint: compatibility_fingerprint.to_owned(),
        schema_version: 8,
        desired_revision: compatibility_fingerprint.to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("project volume metadata")
    .with_resource_id("aws")
    .expect("project volume resource identity");

    VolumeCreateOptions::new("stackctl-bill-aws-data", metadata).expect("project volume request")
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
    resolved_images: Mutex<Vec<String>>,
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
            resolved_images: Mutex::new(Vec::new()),
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

impl ImageResolver for RecordingWorkloadEngine {
    fn ensure_image<'operation>(
        &'operation mut self,
        reference: &'operation ImmutableImageReference,
    ) -> EngineFuture<'operation, ImageId> {
        self.resolved_images
            .lock()
            .expect("resolved images")
            .push(reference.as_str().to_owned());
        Box::pin(async { ImageId::new(format!("sha256:{}", "a".repeat(64))) })
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

fn build_image_metadata() -> ManagedResourceMetadata {
    ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::Build,
        project_id: None,
        compatibility_fingerprint: "sha256:runtime-v1".to_owned(),
        schema_version: 8,
        desired_revision: "sha256:runtime-v1".to_owned(),
        retention: RetentionClass::BuildCache,
    })
    .expect("build image metadata")
}

fn observed_build_image(
    digest_character: char,
    created_at_unix_seconds: i64,
    container_count: i64,
    labels: BTreeMap<String, String>,
) -> ObservedImage {
    ObservedImage::new(
        ImageId::new(format!(
            "sha256:{}",
            digest_character.to_string().repeat(64)
        ))
        .expect("image ID"),
        created_at_unix_seconds,
        container_count,
        labels,
    )
}

struct RecordingImageGarbageCollectionEngine {
    observed: Vec<ObservedImage>,
    removed: Vec<ImageId>,
}

impl ImageDiscovery for RecordingImageGarbageCollectionEngine {
    fn discover_managed_images(&self) -> EngineFuture<'_, Vec<ObservedImage>> {
        Box::pin(async { Ok(self.observed.clone()) })
    }
}

impl ImageManager for RecordingImageGarbageCollectionEngine {
    fn remove_image<'operation>(
        &'operation mut self,
        image: &'operation OwnedImage,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.removed.push(image.id().clone());
            Ok(())
        })
    }
}

fn orphaned_container_resource(
    container_id: &str,
    metadata: &ManagedResourceMetadata,
    orphaned_at_unix_seconds: i64,
) -> ResourceRecord {
    ResourceRecord::new(ResourceRecordOptions {
        resource_id: container_id.to_owned(),
        installation_id: metadata.installation_id().to_owned(),
        kind: metadata.kind().label().to_owned(),
        compatibility_fingerprint: metadata.compatibility_fingerprint().to_owned(),
        project_id: metadata.project_id().map(str::to_owned),
        schema_version: metadata.schema_version(),
        desired_revision: metadata.desired_revision().to_owned(),
        retention: match metadata.retention() {
            RetentionClass::Persistent => ResourceRetention::Persistent,
            RetentionClass::Disposable => ResourceRetention::Disposable,
            RetentionClass::BuildCache => ResourceRetention::BuildCache,
        },
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(orphaned_at_unix_seconds),
    })
    .with_scope_id(metadata.resource_id().expect("container resource identity"))
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

struct RecordingProjectVolumeEngine {
    observed: Vec<ObservedVolume>,
}

impl VolumeDiscovery for RecordingProjectVolumeEngine {
    fn discover_managed_volumes(&self) -> EngineFuture<'_, Vec<ObservedVolume>> {
        Box::pin(async { Ok(self.observed.clone()) })
    }
}

impl VolumeManager for RecordingProjectVolumeEngine {
    fn create_volume<'operation>(
        &'operation mut self,
        _options: &'operation VolumeCreateOptions,
    ) -> EngineFuture<'operation, OwnedVolume> {
        Box::pin(async {
            Err(EngineError::Backend {
                detail: "unexpected project volume creation".to_owned(),
            })
        })
    }

    fn remove_volume<'operation>(
        &'operation mut self,
        _volume: &'operation OwnedVolume,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }
}
