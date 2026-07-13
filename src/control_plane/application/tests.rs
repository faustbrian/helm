use super::{ControlPlane, ProjectSource, plan_project_registry};
use crate::control_plane::state::{SqliteStateStore, StateStore};
use crate::control_plane::{ServiceDeploymentStrategy, resolve_execution_plan};
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
