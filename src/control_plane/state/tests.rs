use super::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EngineProvider,
    EnvironmentLifecycle, InstallationRecord, ManagedEnvironmentRecord,
    ManagedEnvironmentRecordOptions, ProjectRecord, ResourceLifecycle, ResourceRecord,
    ResourceRecordOptions, ResourceRetention, SqliteStateStore, StateStore,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn opening_a_new_store_applies_the_current_schema_atomically() {
    let database_path = temporary_database_path("migration");

    let store = SqliteStateStore::open(&database_path).expect("open state store");

    assert_eq!(store.schema_version().expect("schema version"), 6);
    assert_eq!(store.journal_mode().expect("journal mode"), "wal");

    drop(store);
    remove_database(&database_path);
}

#[test]
fn installation_and_watched_roots_survive_store_restart() {
    let database_path = temporary_database_path("installation");
    let installation = InstallationRecord::new(
        "install-1",
        EngineProvider::Docker,
        "unix:///var/run/docker.sock",
    );

    {
        let mut store = SqliteStateStore::open(&database_path).expect("open state store");
        store
            .initialize_installation(&installation)
            .expect("initialize installation");
        store
            .replace_watched_roots(&[
                PathBuf::from("/work/zeta"),
                PathBuf::from("/work/alpha"),
                PathBuf::from("/work/alpha"),
            ])
            .expect("persist watched roots");
    }

    let store = SqliteStateStore::open(&database_path).expect("reopen state store");

    assert_eq!(
        store.installation().expect("load installation"),
        Some(installation)
    );
    assert_eq!(
        store.watched_roots().expect("load watched roots"),
        vec![PathBuf::from("/work/alpha"), PathBuf::from("/work/zeta")]
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn installation_selection_is_idempotent_but_cannot_silently_change() {
    let database_path = temporary_database_path("installation-conflict");
    let installation = InstallationRecord::new(
        "install-1",
        EngineProvider::Docker,
        "unix:///var/run/docker.sock",
    );
    let replacement = InstallationRecord::new(
        "install-2",
        EngineProvider::Docker,
        "unix:///Users/example/.docker/run/docker.sock",
    );
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");

    store
        .initialize_installation(&installation)
        .expect("initialize installation");
    store
        .initialize_installation(&installation)
        .expect("replay installation");
    let error = store
        .initialize_installation(&replacement)
        .expect_err("reject replacement installation");

    assert_eq!(
        error.to_string(),
        "Stackctl installation is already initialized as 'install-1'; explicit migration is required"
    );
    assert_eq!(
        store.installation().expect("load installation"),
        Some(installation)
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn project_ownership_survives_store_restart() {
    let database_path = temporary_database_path("restart");
    let record = project_record(
        "/work/bill",
        "bill",
        &[
            "bill-app.stackctl.localhost",
            "bill-mailpit.stackctl.localhost",
        ],
    );

    {
        let mut store = SqliteStateStore::open(&database_path).expect("open state store");
        store
            .replace_project(&record)
            .expect("persist project ownership");
    }

    let store = SqliteStateStore::open(&database_path).expect("reopen state store");

    assert_eq!(store.projects().expect("load projects"), vec![record]);

    drop(store);
    remove_database(&database_path);
}

#[test]
fn conflicting_route_ownership_rolls_back_the_entire_project_write() {
    let database_path = temporary_database_path("collision");
    let first = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let conflicting = project_record(
        "/work/archive/bill",
        "bill",
        &[
            "bill-app.stackctl.localhost",
            "bill-mailpit.stackctl.localhost",
        ],
    );
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .replace_project(&first)
        .expect("persist first project");

    let error = store
        .replace_project(&conflicting)
        .expect_err("route ownership conflict");

    assert_eq!(
        error.to_string(),
        "route 'bill-app.stackctl.localhost' is owned by '/work/bill', not '/work/archive/bill'"
    );
    assert_eq!(store.projects().expect("load projects"), vec![first]);

    drop(store);
    remove_database(&database_path);
}

#[test]
fn dropping_an_uncommitted_transaction_leaves_no_partial_project() {
    let database_path = temporary_database_path("interruption");

    {
        let store = SqliteStateStore::open(&database_path).expect("open state store");
        store
            .connection
            .execute_batch(
                "BEGIN IMMEDIATE;\n\
                 INSERT INTO projects (canonical_path, project_name)\n\
                 VALUES ('/work/partial', 'partial');",
            )
            .expect("write uncommitted project");
    }

    let store = SqliteStateStore::open(&database_path).expect("reopen state store");

    assert!(store.projects().expect("load projects").is_empty());

    drop(store);
    remove_database(&database_path);
}

#[test]
fn complete_resource_ownership_survives_store_restart() {
    let database_path = temporary_database_path("resources");
    let resource = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "container-1".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "shared_service".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:desired-v1".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });

    {
        let mut store = SqliteStateStore::open(&database_path).expect("open state store");
        store
            .upsert_resources(std::slice::from_ref(&resource))
            .expect("persist resource ownership");
    }

    let store = SqliteStateStore::open(&database_path).expect("reopen state store");

    assert_eq!(store.resources().expect("load resources"), vec![resource]);

    drop(store);
    remove_database(&database_path);
}

#[test]
fn resource_upsert_rejects_immutable_ownership_drift_atomically() {
    let database_path = temporary_database_path("resource-ownership-conflict");
    let first = resource_record("container-first", "bill", ResourceRetention::Persistent);
    let second = resource_record("container-second", "shop", ResourceRetention::Persistent);
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .upsert_resources(&[first.clone(), second.clone()])
        .expect("persist initial resources");
    let updated_first = ResourceRecord::new(ResourceRecordOptions {
        resource_id: first.resource_id().to_owned(),
        installation_id: first.installation_id().to_owned(),
        kind: first.kind().to_owned(),
        compatibility_fingerprint: first.compatibility_fingerprint().to_owned(),
        project_id: first.project_id().map(str::to_owned),
        schema_version: first.schema_version(),
        desired_revision: "sha256:desired-v2".to_owned(),
        retention: first.retention(),
        lifecycle: ResourceLifecycle::Retained,
        orphaned_at_unix_seconds: Some(20_000),
    });
    let forged_second = ResourceRecord::new(ResourceRecordOptions {
        resource_id: second.resource_id().to_owned(),
        installation_id: "foreign-installation".to_owned(),
        kind: second.kind().to_owned(),
        compatibility_fingerprint: second.compatibility_fingerprint().to_owned(),
        project_id: second.project_id().map(str::to_owned),
        schema_version: second.schema_version(),
        desired_revision: "sha256:forged".to_owned(),
        retention: second.retention(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });

    let error = store
        .upsert_resources(&[updated_first, forged_second])
        .expect_err("ownership drift");

    assert_eq!(
        error.to_string(),
        "resource 'container-second' has immutable ownership metadata that differs from durable state; explicit adoption or migration is required"
    );
    assert_eq!(
        store.resources().expect("unchanged resources"),
        vec![first, second]
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn generic_resource_upsert_cannot_reactivate_orphaned_data() {
    let database_path = temporary_database_path("resource-adoption-required");
    let active = resource_record("container-bill", "bill", ResourceRetention::Persistent);
    let orphaned = ResourceRecord::new(ResourceRecordOptions {
        resource_id: active.resource_id().to_owned(),
        installation_id: active.installation_id().to_owned(),
        kind: active.kind().to_owned(),
        compatibility_fingerprint: active.compatibility_fingerprint().to_owned(),
        project_id: active.project_id().map(str::to_owned),
        schema_version: active.schema_version(),
        desired_revision: active.desired_revision().to_owned(),
        retention: active.retention(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(20_000),
    });
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .upsert_resources(std::slice::from_ref(&orphaned))
        .expect("persist orphan");

    let error = store
        .upsert_resources(std::slice::from_ref(&active))
        .expect_err("implicit adoption");

    assert_eq!(
        error.to_string(),
        "resource 'container-bill' is orphaned or retained; explicit adoption is required before reactivation"
    );
    assert_eq!(store.resources().expect("retained orphan"), vec![orphaned]);

    drop(store);
    remove_database(&database_path);
}

#[test]
fn explicit_resource_adoption_reactivates_only_exact_durable_ownership() {
    let database_path = temporary_database_path("resource-adoption");
    let active = resource_record("container-bill", "bill", ResourceRetention::Persistent);
    let orphaned = ResourceRecord::new(ResourceRecordOptions {
        resource_id: active.resource_id().to_owned(),
        installation_id: active.installation_id().to_owned(),
        kind: active.kind().to_owned(),
        compatibility_fingerprint: active.compatibility_fingerprint().to_owned(),
        project_id: active.project_id().map(str::to_owned),
        schema_version: active.schema_version(),
        desired_revision: active.desired_revision().to_owned(),
        retention: active.retention(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(20_000),
    });
    let adopted = ResourceRecord::new(ResourceRecordOptions {
        resource_id: active.resource_id().to_owned(),
        installation_id: active.installation_id().to_owned(),
        kind: active.kind().to_owned(),
        compatibility_fingerprint: active.compatibility_fingerprint().to_owned(),
        project_id: active.project_id().map(str::to_owned),
        schema_version: active.schema_version(),
        desired_revision: "sha256:adopted-v2".to_owned(),
        retention: active.retention(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .upsert_resources(std::slice::from_ref(&orphaned))
        .expect("persist orphan");

    store
        .adopt_resources(std::slice::from_ref(&adopted))
        .expect("explicit adoption");

    assert_eq!(store.resources().expect("adopted resource"), vec![adopted]);

    drop(store);
    remove_database(&database_path);
}

#[test]
fn explicit_resource_adoption_rejects_missing_or_incompatible_records() {
    let database_path = temporary_database_path("resource-adoption-conflict");
    let existing = resource_record("container-bill", "bill", ResourceRetention::Persistent);
    let incompatible = ResourceRecord::new(ResourceRecordOptions {
        resource_id: existing.resource_id().to_owned(),
        installation_id: existing.installation_id().to_owned(),
        kind: existing.kind().to_owned(),
        compatibility_fingerprint: "sha256:different-runtime".to_owned(),
        project_id: existing.project_id().map(str::to_owned),
        schema_version: existing.schema_version(),
        desired_revision: existing.desired_revision().to_owned(),
        retention: existing.retention(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .upsert_resources(std::slice::from_ref(&existing))
        .expect("persist resource");

    let error = store
        .adopt_resources(std::slice::from_ref(&incompatible))
        .expect_err("incompatible adoption");

    assert_eq!(
        error.to_string(),
        "resource 'container-bill' has immutable ownership metadata that differs from durable state; explicit adoption or migration is required"
    );
    assert_eq!(
        store.resources().expect("unchanged resource"),
        vec![existing]
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn unregistering_a_project_atomically_orphans_only_its_resources() {
    let database_path = temporary_database_path("project-orphan");
    let bill = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let shop = project_record("/work/shop", "shop", &["shop-app.stackctl.localhost"]);
    let bill_resource = resource_record("container-bill", "bill", ResourceRetention::Persistent);
    let shop_resource = resource_record("container-shop", "shop", ResourceRetention::Disposable);
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .replace_projects(&[bill, shop.clone()])
        .expect("persist projects");
    store
        .upsert_resources(&[bill_resource, shop_resource.clone()])
        .expect("persist resources");

    store
        .orphan_project(Path::new("/work/bill"), 12_345)
        .expect("orphan missing project");

    assert_eq!(store.projects().expect("load projects"), vec![shop]);
    assert_eq!(
        store.resources().expect("load resources"),
        vec![
            ResourceRecord::new(ResourceRecordOptions {
                resource_id: "container-bill".to_owned(),
                installation_id: "install-1".to_owned(),
                kind: "application".to_owned(),
                compatibility_fingerprint: "sha256:runtime".to_owned(),
                project_id: Some("bill".to_owned()),
                schema_version: 8,
                desired_revision: "sha256:desired-v1".to_owned(),
                retention: ResourceRetention::Persistent,
                lifecycle: ResourceLifecycle::Orphaned,
                orphaned_at_unix_seconds: Some(12_345),
            }),
            shop_resource,
        ]
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn credentials_remain_stable_redacted_and_durable() {
    let database_path = temporary_database_path("credentials");
    let credential = credential_record("secret-first");

    {
        let mut store = SqliteStateStore::open(&database_path).expect("open state store");
        let inserted = store
            .insert_credential_if_absent(&credential)
            .expect("insert credential");
        let replay = store
            .insert_credential_if_absent(&credential_record("secret-replacement"))
            .expect("reconcile credential");

        assert_eq!(inserted, credential);
        assert_eq!(replay, credential);
        assert!(!format!("{replay:?}").contains("secret-first"));
        assert!(format!("{replay:?}").contains("[REDACTED]"));
    }

    let store = SqliteStateStore::open(&database_path).expect("reopen state store");

    assert_eq!(
        store.credentials().expect("load credentials"),
        vec![credential]
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn unregistering_a_project_disables_credentials_without_deleting_them() {
    let database_path = temporary_database_path("credential-orphan");
    let project = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store.replace_project(&project).expect("persist project");
    store
        .insert_credential_if_absent(&credential_record("secret-first"))
        .expect("insert credential");

    store
        .orphan_project(Path::new("/work/bill"), 12_345)
        .expect("orphan project");

    let credential = store
        .credentials()
        .expect("load credentials")
        .pop()
        .expect("retained credential");
    assert_eq!(credential.lifecycle(), CredentialLifecycle::Disabled);
    assert_eq!(credential.secret(), "secret-first");

    drop(store);
    remove_database(&database_path);
}

#[test]
fn managed_environment_replaces_completely_and_survives_restart() {
    let database_path = temporary_database_path("managed-environment");
    let initial = managed_environment(BTreeMap::from([
        (
            "DB_DATABASE".to_owned(),
            "stackctl_bill_database".to_owned(),
        ),
        ("DB_PASSWORD".to_owned(), "project-secret".to_owned()),
    ]));
    let replacement = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:environment-v2".to_owned(),
        values: BTreeMap::from([(
            "DB_DATABASE".to_owned(),
            "stackctl_bill_database".to_owned(),
        )]),
        lifecycle: EnvironmentLifecycle::Active,
    });

    {
        let mut store = SqliteStateStore::open(&database_path).expect("open state store");
        store
            .replace_managed_environment(&initial)
            .expect("persist environment");
        store
            .replace_managed_environment(&replacement)
            .expect("replace environment");
        assert!(!format!("{initial:?}").contains("project-secret"));
        assert!(format!("{initial:?}").contains("DB_PASSWORD"));
    }

    let store = SqliteStateStore::open(&database_path).expect("reopen state store");

    assert_eq!(
        store
            .managed_environments()
            .expect("load managed environments"),
        vec![replacement]
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn unregistering_a_project_disables_its_managed_environment() {
    let database_path = temporary_database_path("environment-orphan");
    let project = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store.replace_project(&project).expect("persist project");
    store
        .replace_managed_environment(&managed_environment(BTreeMap::from([(
            "DB_PASSWORD".to_owned(),
            "project-secret".to_owned(),
        )])))
        .expect("persist environment");

    store
        .orphan_project(Path::new("/work/bill"), 12_345)
        .expect("orphan project");

    assert_eq!(
        store
            .managed_environments()
            .expect("load managed environment")[0]
            .lifecycle(),
        EnvironmentLifecycle::Disabled
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn version_one_state_migrates_without_losing_project_ownership() {
    let database_path = temporary_database_path("v1-migration");
    let connection = rusqlite::Connection::open(&database_path).expect("open legacy state");
    connection
        .execute_batch(
            "CREATE TABLE projects (\n\
                 canonical_path TEXT PRIMARY KEY NOT NULL,\n\
                 project_name TEXT NOT NULL\n\
             ) STRICT;\n\
             CREATE TABLE route_claims (\n\
                 domain TEXT PRIMARY KEY NOT NULL,\n\
                 canonical_path TEXT NOT NULL\n\
                     REFERENCES projects(canonical_path) ON DELETE CASCADE\n\
             ) STRICT;\n\
             INSERT INTO projects VALUES ('/work/bill', 'bill');\n\
             INSERT INTO route_claims VALUES\n\
                 ('bill-app.stackctl.localhost', '/work/bill');\n\
             PRAGMA user_version = 1;",
        )
        .expect("seed legacy schema");
    drop(connection);

    let store = SqliteStateStore::open(&database_path).expect("migrate state store");

    assert_eq!(store.schema_version().expect("schema version"), 6);
    assert_eq!(
        store.projects().expect("preserved projects"),
        vec![project_record(
            "/work/bill",
            "bill",
            &["bill-app.stackctl.localhost"]
        )]
    );
    assert!(store.resources().expect("new resource table").is_empty());

    drop(store);
    remove_database(&database_path);
}

#[test]
fn version_five_credentials_migrate_without_losing_ownership_or_secrets() {
    let database_path = temporary_database_path("v5-credential-migration");
    let connection = rusqlite::Connection::open(&database_path).expect("open legacy state");
    connection
        .execute_batch(
            "CREATE TABLE credentials (\n\
                 credential_id TEXT PRIMARY KEY NOT NULL,\n\
                 project_id TEXT NOT NULL CHECK(length(project_id) > 0),\n\
                 service_id TEXT NOT NULL CHECK(length(service_id) > 0),\n\
                 username TEXT NOT NULL CHECK(length(username) > 0),\n\
                 secret TEXT NOT NULL CHECK(length(secret) > 0),\n\
                 lifecycle TEXT NOT NULL CHECK(lifecycle IN ('active', 'disabled'))\n\
             ) STRICT;\n\
             CREATE INDEX credentials_project_idx ON credentials(project_id);\n\
             INSERT INTO credentials VALUES (\n\
                 'bill/database/primary', 'bill', 'database',\n\
                 'stackctl_bill', 'secret-first', 'active'\n\
             );\n\
             PRAGMA user_version = 5;",
        )
        .expect("seed version-five credentials");
    drop(connection);

    let store = SqliteStateStore::open(&database_path).expect("migrate state store");

    assert_eq!(store.schema_version().expect("schema version"), 6);
    assert_eq!(
        store.credentials().expect("preserved credentials"),
        vec![credential_record("secret-first")]
    );

    drop(store);
    remove_database(&database_path);
}

fn project_record(path: &str, name: &str, domains: &[&str]) -> ProjectRecord {
    ProjectRecord::new(
        PathBuf::from(path),
        name.to_owned(),
        domains.iter().map(|domain| (*domain).to_owned()).collect(),
    )
}

fn resource_record(
    resource_id: &str,
    project_id: &str,
    retention: ResourceRetention,
) -> ResourceRecord {
    ResourceRecord::new(ResourceRecordOptions {
        resource_id: resource_id.to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "application".to_owned(),
        compatibility_fingerprint: "sha256:runtime".to_owned(),
        project_id: Some(project_id.to_owned()),
        schema_version: 8,
        desired_revision: "sha256:desired-v1".to_owned(),
        retention,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
}

fn credential_record(secret: &str) -> CredentialRecord {
    CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/primary".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "stackctl_bill".to_owned(),
        secret: secret.to_owned(),
        lifecycle: CredentialLifecycle::Active,
    })
}

fn managed_environment(values: BTreeMap<String, String>) -> ManagedEnvironmentRecord {
    ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:environment-v1".to_owned(),
        values,
        lifecycle: EnvironmentLifecycle::Active,
    })
}

fn temporary_database_path(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos();

    std::env::temp_dir().join(format!(
        "stackctl-v8-{name}-{}-{unique}.sqlite3",
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
