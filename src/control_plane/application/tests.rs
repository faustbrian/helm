use super::{ControlPlane, ProjectSource, plan_project_registry};
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EngineProvider,
    EnvironmentLifecycle, InstallationRecord, LogicalResourceRecord, LogicalResourceRecordOptions,
    ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions, ProjectRecord, RecoveryPointRecord,
    RecoveryPointRecordOptions, ResourceLifecycle, ResourceRecord, ResourceRecordOptions,
    ResourceRetention, SqliteStateStore, StateStore,
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
fn installation_deletion_preflight_reads_complete_durable_recovery_state() {
    use crate::control_plane::retention::{
        BackupResourceIdentity, store_backup_artifact_for_identity,
    };

    let database_path = temporary_database_path();
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/database".to_owned(),
        shared_resource_id: "postgres-17".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "postgres_database_and_role".to_owned(),
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
    let backup_root = database_path.with_extension("backups");
    let backup = store_backup_artifact_for_identity(
        &BackupResourceIdentity::from_logical(&logical, "install-1"),
        b"backup data",
        9_000,
        &backup_root,
    )
    .expect("store backup artifact");
    let recovery = RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "backup-42".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        logical_resource_id: "bill/database".to_owned(),
        resource_kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        reference: backup.recovery_point().display().to_string(),
        artifact_sha256: "d9c38b4a49e99d9a64a34bfec2d42ee152283003487e13460a1d6de6fb853473"
            .to_owned(),
        artifact_size_bytes: 11,
        created_at_unix_seconds: 9_000,
        verified_at_unix_seconds: 9_001,
    })
    .expect("valid recovery point");
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .initialize_installation(&InstallationRecord::new(
            "install-1",
            EngineProvider::Docker,
            "unix:///engine.sock",
        ))
        .expect("initialize installation");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("persist logical resource");
    store
        .insert_credential_if_absent(&credential)
        .expect("persist credential");
    store
        .record_recovery_point(&recovery)
        .expect("persist recovery point");
    let mut control_plane = ControlPlane::new(store);

    let plan = control_plane
        .plan_installation_deletion()
        .expect("complete deletion preflight");

    assert_eq!(plan.logical_prunes().len(), 1);
    assert_eq!(plan.logical_prunes()[0].recovery_point_id(), "backup-42");
    let error = control_plane
        .begin_confirmed_installation_deletion(&"0".repeat(64), 10_000)
        .expect_err("stale token must fail before freeze");
    assert!(error.contains("confirmation token"));
    assert_eq!(
        control_plane
            .installation_lifecycle()
            .expect("active lifecycle"),
        Some(crate::control_plane::state::InstallationLifecycle::Active)
    );
    let accepted_json =
        serde_json::to_string(&crate::control_plane::daemon::IpcEventKind::Accepted)
            .expect("accepted event");
    control_plane
        .enqueue_daemon_operation(
            &crate::control_plane::state::DaemonOperationRecord::new(
                crate::control_plane::state::DaemonOperationRecordOptions {
                    operation_id: "backup-in-flight".to_owned(),
                    kind: "project_backup".to_owned(),
                    payload_json: "{\"project\":\"bill\"}".to_owned(),
                    status: crate::control_plane::state::DaemonOperationStatus::Queued,
                    created_at_unix_seconds: 9_500,
                    updated_at_unix_seconds: 9_500,
                },
            ),
            &accepted_json,
            256,
        )
        .expect("queue competing operation");
    let error = control_plane
        .begin_confirmed_installation_deletion(plan.confirmation_token(), 10_000)
        .expect_err("active operation must block deletion freeze");
    assert!(error.contains("backup-in-flight"));
    assert_eq!(
        control_plane
            .installation_lifecycle()
            .expect("active lifecycle"),
        Some(crate::control_plane::state::InstallationLifecycle::Active)
    );
    control_plane
        .transition_daemon_operation(
            crate::control_plane::state::DaemonOperationTransitionOptions {
                operation_id: "backup-in-flight",
                expected: crate::control_plane::state::DaemonOperationStatus::Queued,
                next: crate::control_plane::state::DaemonOperationStatus::Failed,
                updated_at_unix_seconds: 9_600,
                event_kind_json: Some(
                    "{\"kind\":\"failed\",\"code\":\"cancelled\",\"message\":\"test\"}",
                ),
                event_retention_limit: 256,
            },
        )
        .expect("terminalize competing operation");
    control_plane
        .begin_confirmed_installation_deletion(plan.confirmation_token(), 10_000)
        .expect("confirmed deletion transition");
    assert_eq!(
        control_plane
            .installation_lifecycle()
            .expect("deleting lifecycle"),
        Some(crate::control_plane::state::InstallationLifecycle::Deleting)
    );
    remove_database(&database_path);
    std::fs::remove_dir_all(backup_root).expect("remove backup fixture");
}

#[test]
fn installation_deletion_refuses_unprotected_project_volumes() {
    let database_path = temporary_database_path();
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .initialize_installation(&InstallationRecord::new(
            "install-1",
            EngineProvider::Docker,
            "unix:///engine.sock",
        ))
        .expect("initialize installation");
    store
        .upsert_resources(&[ResourceRecord::new(ResourceRecordOptions {
            resource_id: "stackctl-bill-search-data".to_owned(),
            installation_id: "install-1".to_owned(),
            kind: "volume".to_owned(),
            compatibility_fingerprint: "sha256:search-3".to_owned(),
            project_id: Some("bill".to_owned()),
            schema_version: 8,
            desired_revision: "sha256:desired".to_owned(),
            retention: ResourceRetention::Persistent,
            lifecycle: ResourceLifecycle::Retained,
            orphaned_at_unix_seconds: Some(9_000),
        })
        .with_scope_id("search")])
        .expect("persist project volume");
    let control_plane = ControlPlane::new(store);

    let error = control_plane
        .plan_installation_deletion()
        .expect_err("unprotected project volume must block installation deletion");

    assert!(error.contains("stackctl-bill-search-data"));
    assert!(error.contains("no exact verified recovery point"));
    drop(control_plane);
    remove_database(&database_path);
}

#[test]
fn installation_deletion_binds_verified_project_volume_recovery() {
    use crate::control_plane::retention::{
        BackupResourceIdentity, store_backup_artifact_for_identity,
    };
    use sha2::{Digest, Sha256};

    let database_path = temporary_database_path();
    let backup_root = database_path.with_extension("volume-backups");
    let resource = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "stackctl-bill-search-data".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "volume".to_owned(),
        compatibility_fingerprint: "sha256:search-3".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Retained,
        orphaned_at_unix_seconds: Some(9_000),
    })
    .with_scope_id("search");
    let stored = store_backup_artifact_for_identity(
        &BackupResourceIdentity::from_resource(&resource),
        b"volume archive",
        9_100,
        &backup_root,
    )
    .expect("store volume recovery");
    let recovery = RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "volume-backup-42".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "search".to_owned(),
        logical_resource_id: resource.resource_id().to_owned(),
        resource_kind: resource.kind().to_owned(),
        compatibility_fingerprint: resource.compatibility_fingerprint().to_owned(),
        reference: stored.recovery_point().display().to_string(),
        artifact_sha256: hex::encode(Sha256::digest(b"volume archive")),
        artifact_size_bytes: 14,
        created_at_unix_seconds: 9_100,
        verified_at_unix_seconds: 9_100,
    })
    .expect("volume recovery point");
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .initialize_installation(&InstallationRecord::new(
            "install-1",
            EngineProvider::Docker,
            "unix:///engine.sock",
        ))
        .expect("initialize installation");
    store
        .upsert_resources(std::slice::from_ref(&resource))
        .expect("persist project volume");
    store
        .record_recovery_point(&recovery)
        .expect("catalog volume recovery");
    let mut control_plane = ControlPlane::new(store);

    let plan = control_plane
        .plan_installation_deletion()
        .expect("protected project volume plan");
    let [deletion] = plan.volume_deletions() else {
        panic!("expected one volume deletion");
    };
    assert_eq!(deletion.resource_id(), resource.resource_id());
    assert_eq!(deletion.recovery_point_id(), recovery.recovery_point_id());
    control_plane
        .begin_confirmed_installation_deletion(plan.confirmation_token(), 9_200)
        .expect("freeze verified volume deletion");
    assert_eq!(
        control_plane
            .verified_installation_volume_deletions(9_201)
            .expect("reverify cleanup authorization"),
        [resource.resource_id()]
    );
    std::fs::write(stored.artifact_file(), b"tampered volume archive")
        .expect("tamper volume recovery");
    assert!(
        control_plane
            .verified_installation_volume_deletions(9_202)
            .expect_err("tampered cleanup authorization must fail")
            .contains("checksum")
    );

    drop(control_plane);
    remove_database(&database_path);
    std::fs::remove_dir_all(backup_root).expect("remove volume backup fixture");
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
fn project_local_yaml_artifact_lock_resolves_mutable_image_before_planning() {
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        "schema_version: 8\nproject: bill\nservices:\n  app:\n    image: ghcr.io/stackctl/php:8.4\n"
            .to_owned(),
    )
    .with_artifact_lock(
        PathBuf::from("/work/bill/.stackctl.lock.yaml"),
        concat!(
            "schema_version: 1\nimages:\n  app:\n",
            "    source: ghcr.io/stackctl/php:8.4\n",
            "    resolved: ghcr.io/stackctl/php@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"
        )
        .to_owned(),
    );

    let registry = plan_project_registry(&[source]).expect("artifact-locked registry");
    let plan = resolve_execution_plan(&registry).expect("execution plan");

    assert_eq!(
        plan.services()[0].desired().image(),
        Some(concat!(
            "ghcr.io/stackctl/php@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ))
    );
}

#[test]
fn stale_project_artifact_lock_fails_loudly() {
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        "schema_version: 8\nproject: bill\nservices:\n  app:\n    image: ghcr.io/stackctl/php:8.4\n"
            .to_owned(),
    )
    .with_artifact_lock(
        PathBuf::from("/work/bill/.stackctl.lock.yaml"),
        concat!(
            "schema_version: 1\nimages:\n  app:\n",
            "    source: ghcr.io/stackctl/php:8.3\n",
            "    resolved: ghcr.io/stackctl/php@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"
        )
        .to_owned(),
    );

    let error = plan_project_registry(&[source]).expect_err("stale artifact lock");

    assert!(error.to_string().contains("source does not match"));
    assert!(error.to_string().contains(".stackctl.lock.yaml"));
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
