use super::{ControlPlane, ProjectSource, plan_project_registry};
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EnvironmentLifecycle,
    LogicalResourceRecord, LogicalResourceRecordOptions, ManagedEnvironmentRecord,
    ManagedEnvironmentRecordOptions, ProjectRecord, ResourceLifecycle, ResourceRecord,
    ResourceRecordOptions, ResourceRetention, SqliteStateStore, StateStore,
};
use crate::control_plane::{ServiceDeploymentStrategy, resolve_execution_plan};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn repeated_discovery_of_one_canonical_project_is_planned_once() {
    let source = project_source("/work/bill", "bill", "app");

    let registry = plan_project_registry(&[source.clone(), source]).expect("valid registry");

    assert_eq!(registry.projects().len(), 1);
}

#[test]
fn resolved_execution_plan_preserves_project_and_dependency_order() {
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        "schema_version: 8\nproject: bill\nservices:\n  worker:\n    preset: queue-worker\n    depends_on: [app]\n  app:\n    preset: laravel\n    depends_on: [db]\n  db:\n    preset: postgres\n"
            .to_owned(),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");

    let plan = resolve_execution_plan(&registry).expect("resolved execution plan");

    assert_eq!(
        plan.services()
            .iter()
            .map(|service| (
                service.project().as_str(),
                service.service().as_str(),
                service.strategy(),
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                "bill",
                "db",
                ServiceDeploymentStrategy::SharedByCompatibility
            ),
            ("bill", "app", ServiceDeploymentStrategy::ProjectApplication),
            ("bill", "worker", ServiceDeploymentStrategy::ProjectProcess),
        ]
    );
}

#[test]
fn complete_discovered_registry_collision_fails_before_persistence() {
    let first = project_source("/work/bill", "bill", "app");
    let second = project_source("/work/archive/bill", "bill", "app");

    let error = plan_project_registry(&[first, second]).expect_err("route collision");

    assert!(error.to_string().contains("bill-app.stackctl.localhost"));
    assert!(error.to_string().contains("/work/bill"));
    assert!(error.to_string().contains("/work/archive/bill"));
}

#[test]
fn persisted_owner_collision_rolls_back_every_project_in_the_new_batch() {
    let database_path = temporary_database_path();
    let store = SqliteStateStore::open(&database_path).expect("open state store");
    let mut control_plane = ControlPlane::new(store);
    control_plane
        .reconcile_projects(&[project_source("/work/bill", "bill", "app")])
        .expect("persist initial registry");

    let error = control_plane
        .reconcile_projects(&[
            project_source("/work/shop", "shop", "app"),
            project_source("/work/archive/bill", "bill", "app"),
        ])
        .expect_err("persisted ownership conflict");

    assert!(error.to_string().contains("owned by '/work/bill'"));
    drop(control_plane);

    let store = SqliteStateStore::open(&database_path).expect("reopen state store");
    let projects = store.projects().expect("load projects");

    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].canonical_path(), Path::new("/work/bill"));

    drop(store);
    remove_database(&database_path);
}

#[test]
fn complete_discovery_reconciliation_unregisters_missing_projects() {
    let database_path = temporary_database_path();
    let store = SqliteStateStore::open(&database_path).expect("open state store");
    let mut control_plane = ControlPlane::new(store);
    control_plane
        .reconcile_discovered_projects(
            &[
                project_source("/work/bill", "bill", "app"),
                project_source("/work/shop", "shop", "app"),
            ],
            10_000,
        )
        .expect("initial complete registry");

    control_plane
        .reconcile_discovered_projects(&[project_source("/work/shop", "shop", "app")], 12_345)
        .expect("updated complete registry");
    drop(control_plane);

    let store = SqliteStateStore::open(&database_path).expect("reopen state store");
    let projects = store.projects().expect("load projects");
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].canonical_path(), Path::new("/work/shop"));

    drop(store);
    remove_database(&database_path);
}

#[test]
fn explicit_adoption_restores_current_state_without_reactivating_replacement_history() {
    let database_path = temporary_database_path();
    let project = ProjectRecord::new(
        PathBuf::from("/work/bill"),
        "bill".to_owned(),
        vec!["bill-app.stackctl.localhost".to_owned()],
    );
    let current = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "container-current".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "project_application".to_owned(),
        compatibility_fingerprint: "sha256:runtime".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:current".to_owned(),
        retention: ResourceRetention::Disposable,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
    .with_scope_id("app");
    let history = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "container-history".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "project_application".to_owned(),
        compatibility_fingerprint: "sha256:old-runtime".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:old".to_owned(),
        retention: ResourceRetention::Disposable,
        lifecycle: ResourceLifecycle::Retained,
        orphaned_at_unix_seconds: Some(10_000),
    })
    .with_scope_id("app");
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/database".to_owned(),
        shared_resource_id: "postgres-17".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "postgres_database".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        desired_revision: "sha256:database".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "bill".to_owned(),
        secret: "retained-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:environment".to_owned(),
        values: BTreeMap::from([("DB_PASSWORD".to_owned(), "retained-secret".to_owned())]),
        lifecycle: EnvironmentLifecycle::Active,
    });
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store.replace_project(&project).expect("register project");
    store
        .upsert_resources(&[current.clone(), history.clone()])
        .expect("persist physical ownership");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("persist logical ownership");
    store
        .insert_credential_if_absent(&credential)
        .expect("persist credential");
    store
        .replace_managed_environment(&environment)
        .expect("persist environment");
    store
        .orphan_project(project.canonical_path(), 12_345)
        .expect("orphan project");
    store
        .replace_project(&project)
        .expect("register exact adoption target");
    let mut control_plane = ControlPlane::new(store);

    let adopted = control_plane
        .adopt_project(project.canonical_path())
        .expect("adopt retained project state");

    assert_eq!(adopted, "bill");
    drop(control_plane);
    let store = SqliteStateStore::open(&database_path).expect("reopen state store");
    let resources = store.resources().expect("load physical ownership");
    assert_eq!(
        resources
            .iter()
            .find(|resource| resource.resource_id() == current.resource_id())
            .expect("current resource")
            .lifecycle(),
        ResourceLifecycle::Active
    );
    assert_eq!(
        resources
            .iter()
            .find(|resource| resource.resource_id() == history.resource_id())
            .expect("replacement history")
            .lifecycle(),
        ResourceLifecycle::Retained
    );
    assert_eq!(
        store.logical_resources().expect("logical ownership")[0].lifecycle(),
        ResourceLifecycle::Active
    );
    assert_eq!(
        store.credentials().expect("credentials")[0].lifecycle(),
        CredentialLifecycle::Active
    );
    assert_eq!(
        store.managed_environments().expect("environment")[0].lifecycle(),
        EnvironmentLifecycle::Active
    );

    drop(store);
    remove_database(&database_path);
}

fn project_source(path: &str, project: &str, service: &str) -> ProjectSource {
    ProjectSource::new(
        PathBuf::from(path),
        PathBuf::from(path).join(".stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: {project}\nservices:\n  {service}:\n    preset: laravel\n"
        ),
    )
}

fn temporary_database_path() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos();

    std::env::temp_dir().join(format!(
        "stackctl-v8-control-plane-{}-{unique}.sqlite3",
        std::process::id()
    ))
}

fn remove_database(database_path: &Path) {
    for suffix in ["", "-shm", "-wal"] {
        let path = PathBuf::from(format!("{}{suffix}", database_path.display()));
        if path.exists() {
            std::fs::remove_file(path).expect("remove temporary state database");
        }
    }
}
