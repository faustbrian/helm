use super::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EngineProvider,
    EnvironmentLifecycle, InstallationRecord, LogicalResourceRecord, LogicalResourceRecordOptions,
    ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions, MigrationPhase, MigrationRecord,
    MigrationRecordOptions, ProjectAdoptionPlan, ProjectAdoptionPlanOptions, ProjectRecord,
    ResourceLifecycle, ResourceRecord, ResourceRecordOptions, ResourceRetention, SqliteStateStore,
    StateStore,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn opening_a_new_store_applies_the_current_schema_atomically() {
    let database_path = temporary_database_path("migration");

    let store = SqliteStateStore::open(&database_path).expect("open state store");

    assert_eq!(store.schema_version().expect("schema version"), 9);
    assert_eq!(store.journal_mode().expect("journal mode"), "wal");

    drop(store);
    remove_database(&database_path);
}

#[test]
fn migration_journal_survives_restart_and_advances_without_skips() {
    let database_path = temporary_database_path("migration-journal");

    {
        let mut store = SqliteStateStore::open(&database_path).expect("open state store");
        store
            .record_migration(&migration_record(MigrationPhase::Inventoried, 10))
            .expect("record inventory");
        store
            .record_migration(&migration_record(MigrationPhase::BackupVerified, 11))
            .expect("record backup");
        store
            .record_migration(&migration_record(MigrationPhase::TargetProvisioned, 12))
            .expect("record target");
    }

    let store = SqliteStateStore::open(&database_path).expect("reopen state store");

    assert_eq!(
        store.migrations().expect("load migrations"),
        vec![migration_record(MigrationPhase::TargetProvisioned, 12)]
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn migration_journal_rejects_skips_identity_drift_and_terminal_changes() {
    let database_path = temporary_database_path("migration-transitions");
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .record_migration(&migration_record(MigrationPhase::Inventoried, 10))
        .expect("record inventory");

    let skipped = store
        .record_migration(&migration_record(MigrationPhase::TargetProvisioned, 12))
        .expect_err("reject skipped backup");
    assert_eq!(
        skipped.to_string(),
        "migration 'migration-bill' cannot advance from 'inventoried' to 'target_provisioned'"
    );

    let mut drifted = migration_options(MigrationPhase::BackupVerified, 11);
    drifted.target_revision = "sha256:different".to_owned();
    let drifted = MigrationRecord::new(drifted).expect("valid drifted record");
    let drift = store
        .record_migration(&drifted)
        .expect_err("reject migration identity drift");
    assert_eq!(
        drift.to_string(),
        "migration 'migration-bill' immutable identity differs from durable state"
    );

    store
        .record_migration(&migration_record(MigrationPhase::BackupVerified, 11))
        .expect("record verified backup");
    let mut replaced_evidence = migration_options(MigrationPhase::TargetProvisioned, 12);
    replaced_evidence.backup_artifact_sha256 = Some("sha256:replacement".to_owned());
    let replaced_evidence = MigrationRecord::new(replaced_evidence).expect("valid replacement");
    let evidence = store
        .record_migration(&replaced_evidence)
        .expect_err("reject replaced backup evidence");
    assert_eq!(
        evidence.to_string(),
        "migration 'migration-bill' durable evidence cannot be replaced"
    );

    let stale = store
        .record_migration(&migration_record(MigrationPhase::TargetProvisioned, 9))
        .expect_err("reject stale checkpoint");
    assert_eq!(
        stale.to_string(),
        "migration 'migration-bill' update time predates durable state"
    );

    for (phase, updated_at) in [
        (MigrationPhase::TargetProvisioned, 12),
        (MigrationPhase::DataRestored, 13),
        (MigrationPhase::TargetVerified, 14),
        (MigrationPhase::Cutover, 15),
        (MigrationPhase::Confirmed, 16),
    ] {
        store
            .record_migration(&migration_record(phase, updated_at))
            .expect("advance migration");
    }

    let terminal = store
        .record_migration(&migration_record(MigrationPhase::RolledBack, 17))
        .expect_err("reject terminal transition");
    assert_eq!(
        terminal.to_string(),
        "migration 'migration-bill' cannot advance from 'confirmed' to 'rolled_back'"
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn migration_records_require_proof_before_destructive_phases() {
    let mut options = migration_options(MigrationPhase::BackupVerified, 11);
    options.backup_artifact_sha256 = None;
    options.backup_artifact_size_bytes = None;
    let backup_error = MigrationRecord::new(options).expect_err("missing backup proof");
    assert_eq!(
        backup_error.to_string(),
        "migration phase 'backup_verified' requires verified backup evidence"
    );

    let mut options = migration_options(MigrationPhase::TargetProvisioned, 12);
    options.target_resource_id = None;
    let target_error = MigrationRecord::new(options).expect_err("missing target identity");
    assert_eq!(
        target_error.to_string(),
        "migration phase 'target_provisioned' requires a target resource identity"
    );

    let mut options = migration_options(MigrationPhase::Cutover, 15);
    options.rollback_reference = None;
    let rollback_error = MigrationRecord::new(options).expect_err("missing rollback material");
    assert_eq!(
        rollback_error.to_string(),
        "migration phase 'cutover' requires retained rollback material"
    );
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
fn logical_resources_persist_and_reference_count_only_active_consumers() {
    let database_path = temporary_database_path("logical-resources");
    let bill = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let shop = project_record("/work/shop", "shop", &["shop-app.stackctl.localhost"]);
    let bill_database = logical_resource_record("bill/database", "bill", "database");
    let shop_database = logical_resource_record("shop/database", "shop", "database");
    let retained = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "old/database".to_owned(),
        shared_resource_id: "postgres-shared-17".to_owned(),
        project_id: "old".to_owned(),
        service_id: "database".to_owned(),
        kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        desired_revision: "sha256:desired-v1".to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(10_000),
    });
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .replace_projects(&[bill, shop])
        .expect("persist projects");
    store
        .upsert_logical_resources(&[
            bill_database.clone(),
            shop_database.clone(),
            retained.clone(),
        ])
        .expect("persist logical resources");

    assert_eq!(
        store
            .active_logical_reference_count("postgres-shared-17")
            .expect("active references"),
        2
    );
    assert_eq!(
        store.logical_resources().expect("logical resources"),
        vec![bill_database, retained, shop_database]
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn orphaning_a_project_releases_its_active_shared_service_reference() {
    let database_path = temporary_database_path("logical-resource-orphan");
    let bill = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let logical = logical_resource_record("bill/database", "bill", "database");
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store.replace_project(&bill).expect("persist project");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("persist logical resource");

    store
        .orphan_project(Path::new("/work/bill"), 12_345)
        .expect("orphan project");

    let retained = store
        .logical_resources()
        .expect("retained logical resources")
        .pop()
        .expect("logical resource");
    assert_eq!(retained.lifecycle(), ResourceLifecycle::Orphaned);
    assert_eq!(retained.orphaned_at_unix_seconds(), Some(12_345));
    assert_eq!(
        store
            .active_logical_reference_count("postgres-shared-17")
            .expect("released references"),
        0
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn logical_resource_reconciliation_cannot_reassign_or_implicitly_adopt_tenants() {
    let database_path = temporary_database_path("logical-resource-ownership");
    let project = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let logical = logical_resource_record("bill/database", "bill", "database");
    let forged = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: logical.logical_resource_id().to_owned(),
        shared_resource_id: "foreign-postgres".to_owned(),
        project_id: logical.project_id().to_owned(),
        service_id: logical.service_id().to_owned(),
        kind: logical.kind().to_owned(),
        compatibility_fingerprint: logical.compatibility_fingerprint().to_owned(),
        desired_revision: logical.desired_revision().to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store.replace_project(&project).expect("persist project");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("persist logical resource");

    let error = store
        .upsert_logical_resources(std::slice::from_ref(&forged))
        .expect_err("logical ownership drift");
    assert_eq!(
        error.to_string(),
        "logical resource 'bill/database' has immutable ownership metadata that differs from durable state; explicit adoption or migration is required"
    );

    store
        .orphan_project(project.canonical_path(), 12_345)
        .expect("orphan project");
    let error = store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect_err("implicit logical adoption");
    assert_eq!(
        error.to_string(),
        "project 'bill' has disabled managed state; explicit adoption is required before reactivation"
    );

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
fn complete_registry_reconciliation_orphans_projects_missing_from_the_scan() {
    let database_path = temporary_database_path("complete-registry-reconcile");
    let bill = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let shop = project_record("/work/shop", "shop", &["shop-app.stackctl.localhost"]);
    let bill_resource = resource_record("container-bill", "bill", ResourceRetention::Persistent);
    let bill_logical = logical_resource_record("bill/database", "bill", "database");
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .replace_projects(&[bill, shop.clone()])
        .expect("persist initial registry");
    store
        .upsert_resources(std::slice::from_ref(&bill_resource))
        .expect("persist resource");
    store
        .upsert_logical_resources(std::slice::from_ref(&bill_logical))
        .expect("persist logical resource");
    store
        .insert_credential_if_absent(&credential_record("secret-first"))
        .expect("persist credential");
    store
        .replace_managed_environment(&managed_environment(BTreeMap::new()))
        .expect("persist environment");

    store
        .reconcile_project_registry(std::slice::from_ref(&shop), 12_345)
        .expect("reconcile complete registry");

    assert_eq!(store.projects().expect("remaining projects"), vec![shop]);
    assert_eq!(
        store.resources().expect("orphaned resources")[0].lifecycle(),
        ResourceLifecycle::Orphaned
    );
    assert_eq!(
        store
            .logical_resources()
            .expect("orphaned logical resources")[0]
            .lifecycle(),
        ResourceLifecycle::Orphaned
    );
    assert_eq!(
        store.credentials().expect("disabled credentials")[0].lifecycle(),
        CredentialLifecycle::Disabled
    );
    assert_eq!(
        store.managed_environments().expect("disabled environment")[0].lifecycle(),
        EnvironmentLifecycle::Disabled
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
fn replacing_managed_environment_cannot_implicitly_reactivate_an_orphan() {
    let database_path = temporary_database_path("environment-adoption-required");
    let project = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let environment = managed_environment(BTreeMap::from([(
        "DB_PASSWORD".to_owned(),
        "project-secret".to_owned(),
    )]));
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store.replace_project(&project).expect("persist project");
    store
        .replace_managed_environment(&environment)
        .expect("persist environment");
    store
        .orphan_project(Path::new("/work/bill"), 12_345)
        .expect("orphan project");

    let error = store
        .replace_managed_environment(&environment)
        .expect_err("implicit environment adoption");

    assert_eq!(
        error.to_string(),
        "project 'bill' has disabled managed state; explicit adoption is required before reactivation"
    );
    assert_eq!(
        store.managed_environments().expect("retained environment")[0].lifecycle(),
        EnvironmentLifecycle::Disabled
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn explicit_project_adoption_reactivates_resources_credentials_and_environment_atomically() {
    let database_path = temporary_database_path("complete-project-adoption");
    let project = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let resource = resource_record("container-bill", "bill", ResourceRetention::Persistent);
    let logical = logical_resource_record("bill/database", "bill", "database");
    let credential = credential_record("secret-first");
    let environment = managed_environment(BTreeMap::from([(
        "DB_PASSWORD".to_owned(),
        "secret-first".to_owned(),
    )]));
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .replace_project(&project)
        .expect("persist original project");
    store
        .upsert_resources(std::slice::from_ref(&resource))
        .expect("persist resource");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("persist logical resource");
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
        .expect("register adoption target");
    let incompatible_resource = ResourceRecord::new(ResourceRecordOptions {
        resource_id: resource.resource_id().to_owned(),
        installation_id: resource.installation_id().to_owned(),
        kind: resource.kind().to_owned(),
        compatibility_fingerprint: "sha256:different-runtime".to_owned(),
        project_id: resource.project_id().map(str::to_owned),
        schema_version: resource.schema_version(),
        desired_revision: resource.desired_revision().to_owned(),
        retention: resource.retention(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let incompatible = ProjectAdoptionPlan::new(ProjectAdoptionPlanOptions {
        canonical_path: project.canonical_path().to_path_buf(),
        project_id: "bill".to_owned(),
        resources: vec![incompatible_resource],
        logical_resources: vec![logical.clone()],
        credential_ids: vec![credential.credential_id().to_owned()],
        environment_revision: environment.revision().to_owned(),
    })
    .expect("incompatible adoption plan");
    let error = store
        .adopt_project(&incompatible)
        .expect_err("incompatible resource adoption");
    assert_eq!(
        error.to_string(),
        "resource 'container-bill' has immutable ownership metadata that differs from durable state; explicit adoption or migration is required"
    );
    let incomplete = ProjectAdoptionPlan::new(ProjectAdoptionPlanOptions {
        canonical_path: project.canonical_path().to_path_buf(),
        project_id: "bill".to_owned(),
        resources: vec![resource.clone()],
        logical_resources: vec![logical.clone()],
        credential_ids: vec!["bill/database/missing".to_owned()],
        environment_revision: environment.revision().to_owned(),
    })
    .expect("incomplete adoption plan");
    let error = store
        .adopt_project(&incomplete)
        .expect_err("missing retained credential");
    assert_eq!(
        error.to_string(),
        "project 'bill' adoption does not match retained state: credential 'bill/database/missing' is missing or owned by another project"
    );
    assert_eq!(
        store.resources().expect("still orphaned")[0].lifecycle(),
        ResourceLifecycle::Orphaned
    );
    assert_eq!(
        store.managed_environments().expect("still disabled")[0].lifecycle(),
        EnvironmentLifecycle::Disabled
    );
    let adoption = ProjectAdoptionPlan::new(ProjectAdoptionPlanOptions {
        canonical_path: project.canonical_path().to_path_buf(),
        project_id: "bill".to_owned(),
        resources: vec![resource.clone()],
        logical_resources: vec![logical.clone()],
        credential_ids: vec![credential.credential_id().to_owned()],
        environment_revision: environment.revision().to_owned(),
    })
    .expect("project adoption plan");

    store.adopt_project(&adoption).expect("adopt project state");

    assert_eq!(store.resources().expect("active resources"), vec![resource]);
    assert_eq!(
        store.logical_resources().expect("active logical resources"),
        vec![logical]
    );
    assert_eq!(
        store.credentials().expect("active credentials")[0].lifecycle(),
        CredentialLifecycle::Active
    );
    assert_eq!(
        store.managed_environments().expect("active environment")[0].lifecycle(),
        EnvironmentLifecycle::Active
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

    assert_eq!(store.schema_version().expect("schema version"), 9);
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

    assert_eq!(store.schema_version().expect("schema version"), 9);
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

fn logical_resource_record(
    logical_resource_id: &str,
    project_id: &str,
    service_id: &str,
) -> LogicalResourceRecord {
    LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: logical_resource_id.to_owned(),
        shared_resource_id: "postgres-shared-17".to_owned(),
        project_id: project_id.to_owned(),
        service_id: service_id.to_owned(),
        kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        desired_revision: "sha256:desired-v1".to_owned(),
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

fn migration_record(phase: MigrationPhase, updated_at_unix_seconds: i64) -> MigrationRecord {
    MigrationRecord::new(migration_options(phase, updated_at_unix_seconds))
        .expect("valid migration record")
}

fn migration_options(
    phase: MigrationPhase,
    updated_at_unix_seconds: i64,
) -> MigrationRecordOptions {
    MigrationRecordOptions {
        migration_id: "migration-bill".to_owned(),
        project_id: "bill".to_owned(),
        source_revision: "sha256:v7".to_owned(),
        target_revision: "sha256:v8".to_owned(),
        source_compatibility_fingerprint: "sha256:postgres-16".to_owned(),
        target_compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        phase,
        backup_reference: (phase >= MigrationPhase::BackupVerified)
            .then(|| "backup:resource-1/40000".to_owned()),
        backup_artifact_sha256: (phase >= MigrationPhase::BackupVerified)
            .then(|| "sha256:backup".to_owned()),
        backup_artifact_size_bytes: (phase >= MigrationPhase::BackupVerified).then_some(1_024),
        target_resource_id: (phase >= MigrationPhase::TargetProvisioned)
            .then(|| "postgres-shared-17".to_owned()),
        rollback_reference: (phase >= MigrationPhase::Cutover)
            .then(|| "retained:v7-resource".to_owned()),
        updated_at_unix_seconds,
    }
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
