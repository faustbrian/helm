use super::{
    ApplicationContainerPlan, ApplicationContainerPlanOptions, ApplicationContainerRequestOptions,
    ProjectProcessPlan, ProjectProcessPlanOptions, ProjectProcessRequestOptions,
    RuntimeEnvironment, RuntimeEnvironmentOptions, application_container_request,
    project_process_request,
};
use crate::control_plane::ProjectIdentity;
use crate::control_plane::engine::{
    ContainerRestartPolicy, ManagedResourceMetadata, ManagedResourceMetadataOptions, ResourceKind,
    RetentionClass,
};
use crate::control_plane::state::{
    EnvironmentLifecycle, ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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
fn project_workers_materialize_as_supervised_private_linux_containers() {
    let project =
        ProjectIdentity::resolve(Some("bill"), Path::new("/work/bill")).expect("project identity");
    let service =
        crate::control_plane::ServiceIdentity::new("queue-worker").expect("service identity");
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
    .expect("worker metadata");

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
    let service = crate::control_plane::ServiceIdentity::new("worker").expect("service identity");

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

fn application_options(project: &str, path: &str) -> ApplicationContainerPlanOptions {
    ApplicationContainerPlanOptions {
        project: ProjectIdentity::resolve(Some(project), Path::new(path))
            .expect("project identity"),
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
