use super::{ProjectDiscoveryOptions, reconcile_watched_roots};
use crate::control_plane::application::ControlPlane;
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EnvironmentLifecycle,
    LogicalResourceRecord, LogicalResourceRecordOptions, ManagedEnvironmentRecord,
    ManagedEnvironmentRecordOptions, ResourceLifecycle, ResourceRecord, ResourceRecordOptions,
    ResourceRetention, SqliteStateStore, StateStore,
};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn watched_configuration_restoration_and_rename_reactivate_exact_owned_state() {
    let root = temporary_directory();
    let original = root.join("bill");
    let renamed = root.join("bill-renamed");
    std::fs::create_dir(&original).expect("create original project directory");
    write_configuration(&original);
    let database_path = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database_path).expect("open lifecycle state");
    store
        .replace_watched_roots(std::slice::from_ref(&root))
        .expect("persist lifecycle watched root");
    let mut control_plane = ControlPlane::new(store);

    let initial = reconcile_watched_roots(
        &mut control_plane,
        ProjectDiscoveryOptions::bounded_defaults(),
        10_000,
    )
    .expect("discover initial project configuration");
    assert!(initial.was_applied());
    drop(control_plane);

    let mut store = SqliteStateStore::open(&database_path).expect("reopen lifecycle state");
    seed_retained_project_state(&mut store);
    drop(store);

    std::fs::remove_file(original.join(".stackctl.yaml")).expect("remove project configuration");
    let store = SqliteStateStore::open(&database_path).expect("reopen removal state");
    let mut control_plane = ControlPlane::new(store);
    let removed = reconcile_watched_roots(
        &mut control_plane,
        ProjectDiscoveryOptions::bounded_defaults(),
        12_345,
    )
    .expect("reconcile removed project configuration");
    assert!(removed.was_applied());
    drop(control_plane);
    assert_retained_state(&database_path, 12_345);

    write_configuration(&original);
    let original = original.canonicalize().expect("canonical restored project");
    let store = SqliteStateStore::open(&database_path).expect("reopen restoration state");
    let mut control_plane = ControlPlane::new(store);
    let restored = reconcile_watched_roots(
        &mut control_plane,
        ProjectDiscoveryOptions::bounded_defaults(),
        15_000,
    )
    .expect("rediscover restored project configuration");
    assert!(restored.was_applied());
    drop(control_plane);
    assert_active_state(&database_path, &original);

    std::fs::rename(&original, &renamed).expect("atomically rename project directory");
    let renamed = renamed.canonicalize().expect("canonical renamed project");
    let store = SqliteStateStore::open(&database_path).expect("reopen rename state");
    let mut control_plane = ControlPlane::new(store);
    let moved = reconcile_watched_roots(
        &mut control_plane,
        ProjectDiscoveryOptions::bounded_defaults(),
        20_000,
    )
    .expect("reconcile atomic project directory rename");
    assert!(moved.was_applied());
    drop(control_plane);
    assert_active_state(&database_path, &renamed);

    std::fs::remove_dir_all(root).expect("remove configuration lifecycle fixture");
}

fn seed_retained_project_state(store: &mut SqliteStateStore) {
    let resource = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "bill-app-container".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "project_application".to_owned(),
        compatibility_fingerprint: "sha256:application".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:application-v1".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
    .with_scope_id("app");
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/database/postgresql".to_owned(),
        shared_resource_id: "postgres-18-data".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-18".to_owned(),
        desired_revision: "sha256:database-v1".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/postgresql".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "bill_database".to_owned(),
        secret: "stable-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:environment-v1".to_owned(),
        values: BTreeMap::from([("DB_PASSWORD".to_owned(), "stable-secret".to_owned())]),
        lifecycle: EnvironmentLifecycle::Active,
    });
    store
        .upsert_resources(std::slice::from_ref(&resource))
        .expect("persist project resource");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("persist project logical resource");
    store
        .insert_credential_if_absent(&credential)
        .expect("persist project credential");
    store
        .replace_managed_environment(&environment)
        .expect("persist project environment");
}

fn assert_retained_state(database_path: &std::path::Path, orphaned_at: i64) {
    let store = SqliteStateStore::open(database_path).expect("inspect retained lifecycle state");
    assert!(store.projects().expect("load retained projects").len() <= 1);
    let resource = &store.resources().expect("load retained resources")[0];
    assert_eq!(resource.lifecycle(), ResourceLifecycle::Orphaned);
    assert_eq!(resource.orphaned_at_unix_seconds(), Some(orphaned_at));
    let logical = &store
        .logical_resources()
        .expect("load retained logical resources")[0];
    assert_eq!(logical.lifecycle(), ResourceLifecycle::Orphaned);
    assert_eq!(logical.orphaned_at_unix_seconds(), Some(orphaned_at));
    let credential = &store.credentials().expect("load retained credentials")[0];
    assert_eq!(credential.lifecycle(), CredentialLifecycle::Disabled);
    assert_eq!(credential.secret(), "stable-secret");
    assert_eq!(
        store
            .managed_environments()
            .expect("load retained environments")[0]
            .lifecycle(),
        EnvironmentLifecycle::Disabled
    );
}

fn assert_active_state(database_path: &std::path::Path, project_path: &std::path::Path) {
    let store = SqliteStateStore::open(database_path).expect("inspect active lifecycle state");
    let projects = store.projects().expect("load active projects");
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].canonical_path(), project_path);
    assert_eq!(
        store.resources().expect("load active resources")[0].lifecycle(),
        ResourceLifecycle::Active
    );
    assert_eq!(
        store
            .logical_resources()
            .expect("load active logical resources")[0]
            .lifecycle(),
        ResourceLifecycle::Active
    );
    let credential = &store.credentials().expect("load active credentials")[0];
    assert_eq!(credential.lifecycle(), CredentialLifecycle::Active);
    assert_eq!(credential.secret(), "stable-secret");
    assert_eq!(
        store
            .managed_environments()
            .expect("load active environments")[0]
            .lifecycle(),
        EnvironmentLifecycle::Active
    );
}

fn write_configuration(directory: &std::path::Path) {
    std::fs::write(
        directory.join(".stackctl.yaml"),
        "schema_version: 8\nproject: bill\nservices:\n  app:\n    preset: laravel\n",
    )
    .expect("write project configuration");
    std::fs::write(
        directory.join(".stackctl.lock.yaml"),
        concat!(
            "schema_version: 1\ncatalog_revision: 2026-07-15.3\nimages:\n",
            "  app:\n    source: preset:laravel\n",
            "    resolved: dunglas/frankenphp@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"
        ),
    )
    .expect("write project artifact lock");
}

fn temporary_directory() -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must follow the Unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "stackctl-configuration-lifecycle-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir(&path).expect("create configuration lifecycle fixture");

    path
}
