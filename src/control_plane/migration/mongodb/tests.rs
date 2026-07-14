use super::{
    EngineV7MongoDbSourceRetirement, V7MongoDbCredential, V7MongoDbMigrationProvider,
    V7MongoDbMigrationProviderOptions, V7MongoDbSourceRetirement,
};
use crate::control_plane::engine::{
    CommandExecutionId, CommandExecutor, CommandRequest, CommandSession, CommandStatus,
    ContainerId, ContainerLogStream, EngineFuture, LogChunk, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ObservedContainer, OwnedContainer, ResourceKind,
    RetentionClass, V7ContainerCommandExecutor, V7ContainerCommandTarget, V7ContainerRetirement,
    V7ContainerRetirementTarget, reconstruct_owned_container,
};
use crate::control_plane::migration::{
    MigrationFuture, V7LogicalDataMigrationSource, V7LogicalDataMigrationSourceOptions,
    V7MigrationAdapterTarget, V7RecoverableMigrationProvider,
};
use crate::control_plane::shared_infrastructure::{CredentialSecret, MongoDbLogicalResourcePlan};
use crate::control_plane::state::{
    AcceptedV7InventoryRecord, AcceptedV7InventoryRecordOptions, CredentialLifecycle,
    CredentialRecord, CredentialRecordOptions, LogicalResourceRecord, LogicalResourceRecordOptions,
    ResourceLifecycle, V7MigrationAdapterCheckpoint,
};
use futures_util::stream;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, duplex};

#[cfg(unix)]
#[test]
fn v7_mongodb_provider_is_namespace_safe_replayable_and_confirmed() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("v7 MongoDB provider runtime");
    let root = backup_root("v7-mongodb-provider");
    let dump = b"mongodb archive bytes".to_vec();
    let executor = RecordingV7MongoDbExecutor::new(dump.clone());
    let accepted = accepted_inventory();
    let source = source();
    let source_credential = V7MongoDbCredential::new("laravel", "legacy-secret", "admin")
        .expect("v7 MongoDB credential");
    let target = target_logical_resource();
    let target_credential = target_credential();
    let target_plan = MongoDbLogicalResourcePlan::new(
        "bill",
        "database",
        CredentialSecret::new("project-secret".to_owned()),
        CredentialSecret::new("root-secret".to_owned()),
    )
    .expect("MongoDB target plan");
    let administrator = administrator();
    let target_container = owned_target_container();
    let mut retirement = RecordingRetirement::default();
    let retired = Arc::clone(&retirement.called);
    let options = V7MongoDbMigrationProviderOptions {
        accepted: &accepted,
        source: &source,
        source_credential: &source_credential,
        target_container: &target_container,
        target_logical_resource: &target,
        target_credential: &target_credential,
        target_plan: &target_plan,
        administrator: &administrator,
        installation_id: "install-1",
        backup_root: &root,
        created_at_unix_seconds: 63_000,
        verified_at_unix_seconds: 63_001,
        timeout: Duration::from_secs(30),
    };
    let debug = format!("{options:?}");
    assert!(!debug.contains("legacy-secret"));
    assert!(!debug.contains("project-secret"));
    assert!(!debug.contains("root-secret"));
    let mut provider = V7MongoDbMigrationProvider::new(&executor, &mut retirement, options)
        .expect("v7 MongoDB provider");

    let backup = runtime
        .block_on(provider.backup_source(&source))
        .expect("backup accepted v7 MongoDB source");
    assert_eq!(
        std::fs::read(Path::new(backup.reference()).join("artifact.bin"))
            .expect("v7 MongoDB backup artifact"),
        dump
    );
    let checkpoint = V7MigrationAdapterCheckpoint::pending(
        "service/database",
        "mongodb-logical-database",
        true,
        62_999,
    )
    .expect("pending v7 MongoDB checkpoint")
    .with_recovery_verified(
        backup.reference(),
        backup.artifact_sha256(),
        backup.artifact_size_bytes(),
        63_001,
    )
    .expect("recovery-verified v7 MongoDB checkpoint");

    let artifact = Path::new(backup.reference()).join("artifact.bin");
    std::fs::write(&artifact, b"tampered").expect("tamper v7 MongoDB backup");
    let error = runtime
        .block_on(provider.restore_and_verify_target(&source, &checkpoint))
        .expect_err("tampered recovery must block target mutation");
    assert!(error.to_string().contains("checksum does not match"));
    assert_eq!(executor.v8_starts.load(Ordering::Acquire), 0);
    std::fs::write(&artifact, &dump).expect("restore v7 MongoDB backup fixture");

    let expected =
        V7MigrationAdapterTarget::resource("mongodb:bill/database:stackctl_bill_database")
            .expect("expected MongoDB target");
    assert_eq!(
        runtime
            .block_on(provider.restore_and_verify_target(&source, &checkpoint))
            .expect("prepare v8 MongoDB target"),
        expected
    );
    assert_eq!(
        runtime
            .block_on(provider.restore_and_verify_target(&source, &checkpoint))
            .expect("replay v8 MongoDB target preparation"),
        expected
    );
    assert_eq!(executor.v8_starts.load(Ordering::Acquire), 8);
    runtime
        .block_on(provider.verify_target(&source, "mongodb:bill/database:stackctl_bill_database"))
        .expect("reverify v8 MongoDB target");
    runtime
        .block_on(provider.verify_source(&source))
        .expect("verify retained v7 MongoDB source");
    assert!(!retired.load(Ordering::Acquire));
    runtime
        .block_on(provider.retire_source(&source))
        .expect("retire confirmed v7 MongoDB source");
    assert!(retired.load(Ordering::Acquire));

    let v7_requests = executor.v7_requests.lock().expect("v7 MongoDB requests");
    let backup_request = &v7_requests[0];
    assert!(!format!("{backup_request:?}").contains("legacy-secret"));
    assert_eq!(
        backup_request
            .environment()
            .get("STACKCTL_MONGODB_DATABASE"),
        Some(&"legacy_bill".to_owned())
    );
    {
        let restore_request = executor
            .restore_request
            .lock()
            .expect("MongoDB restore request");
        let restore_request = restore_request.as_ref().expect("recorded restore request");
        assert_eq!(
            restore_request.environment().get("STACKCTL_MONGODB_SOURCE"),
            Some(&"legacy_bill".to_owned())
        );
        assert_eq!(
            restore_request.environment().get("STACKCTL_MONGODB_TARGET"),
            Some(&"stackctl_bill_database".to_owned())
        );
    }
    assert_eq!(
        executor
            .restored_input
            .lock()
            .expect("restored MongoDB input")
            .as_slice(),
        dump
    );
    drop(v7_requests);
    drop(provider);
    std::fs::remove_dir_all(root).expect("remove v7 MongoDB fixture");
}

#[test]
fn mongodb_target_plan_binds_both_durable_secrets() {
    let plan = MongoDbLogicalResourcePlan::new(
        "bill",
        "database",
        CredentialSecret::new("project-secret".to_owned()),
        CredentialSecret::new("root-secret".to_owned()),
    )
    .expect("MongoDB target plan");
    let credential = target_credential();

    assert!(plan.matches_credential(&credential, "root-secret"));
    assert!(!plan.matches_credential(&credential, "other-root-secret"));
    let mismatched = CredentialRecord::new(CredentialRecordOptions {
        credential_id: credential.credential_id().to_owned(),
        project_id: credential.project_id().map(str::to_owned),
        service_id: credential.service_id().to_owned(),
        username: credential.username().to_owned(),
        secret: "other-project-secret".to_owned(),
        lifecycle: credential.lifecycle(),
    });
    assert!(!plan.matches_credential(&mismatched, "root-secret"));
}

#[test]
fn engine_v7_mongodb_retirement_forwards_the_accepted_resource_set() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("v7 MongoDB retirement runtime");
    let source = source();
    let mut engine = RecordingContainerRetirement::default();
    let mut retirement = EngineV7MongoDbSourceRetirement::new(&mut engine);

    runtime
        .block_on(retirement.retire_source(&source))
        .expect("retire accepted v7 MongoDB resources");

    let target = engine.target.into_inner().expect("retirement target");
    assert_eq!(
        target.expect("recorded retirement").named_volumes(),
        ["bill-database-data"]
    );
}

#[derive(Default)]
struct RecordingRetirement {
    called: Arc<AtomicBool>,
}

impl V7MongoDbSourceRetirement for RecordingRetirement {
    fn retire_source<'operation>(
        &'operation mut self,
        source: &'operation V7LogicalDataMigrationSource,
    ) -> MigrationFuture<'operation, ()> {
        assert_eq!(source, &self::source());
        self.called.store(true, Ordering::Release);
        Box::pin(async { Ok(()) })
    }
}

#[derive(Default)]
struct RecordingContainerRetirement {
    target: Mutex<Option<V7ContainerRetirementTarget>>,
}

impl V7ContainerRetirement for RecordingContainerRetirement {
    fn retire_v7_container<'operation>(
        &'operation mut self,
        target: &'operation V7ContainerRetirementTarget,
    ) -> EngineFuture<'operation, ()> {
        *self.target.lock().expect("retirement target") = Some(target.clone());
        Box::pin(async { Ok(()) })
    }
}

struct RecordingV7MongoDbExecutor {
    dump: Vec<u8>,
    v7_requests: Mutex<Vec<CommandRequest>>,
    restore_request: Mutex<Option<CommandRequest>>,
    restored_input: Arc<Mutex<Vec<u8>>>,
    input_complete: Arc<AtomicBool>,
    v8_starts: AtomicUsize,
}

impl RecordingV7MongoDbExecutor {
    fn new(dump: Vec<u8>) -> Self {
        Self {
            dump,
            v7_requests: Mutex::new(Vec::new()),
            restore_request: Mutex::new(None),
            restored_input: Arc::new(Mutex::new(Vec::new())),
            input_complete: Arc::new(AtomicBool::new(true)),
            v8_starts: AtomicUsize::new(0),
        }
    }

    fn start_session(
        &self,
        container_id: ContainerId,
        request: &CommandRequest,
    ) -> EngineFuture<'_, CommandSession> {
        let environment = request.environment();
        let uri = environment
            .get("STACKCTL_MONGODB_URI")
            .map(String::as_str)
            .unwrap_or_default();
        let output = if environment.contains_key("STACKCTL_MONGODB_DATABASE") {
            self.dump.clone()
        } else if uri.contains("legacy_bill") {
            b"legacy_bill\nlaravel\n1\n".to_vec()
        } else if uri.contains("stackctl_bill_database") {
            b"stackctl_bill_database\nst_bill_database\n1\n".to_vec()
        } else {
            Vec::new()
        };
        let capture_restore = environment.contains_key("STACKCTL_MONGODB_SOURCE");
        if capture_restore {
            *self.restore_request.lock().expect("restore request") = Some(request.clone());
        }
        let restored_input = Arc::clone(&self.restored_input);
        let input_complete = Arc::clone(&self.input_complete);
        input_complete.store(false, Ordering::Release);

        Box::pin(async move {
            let (writer, mut reader) = duplex(64 * 1024);
            tokio::spawn(async move {
                let mut input = Vec::new();
                reader
                    .read_to_end(&mut input)
                    .await
                    .expect("drain v7 MongoDB command input");
                if capture_restore {
                    *restored_input.lock().expect("restored input") = input;
                }
                input_complete.store(true, Ordering::Release);
            });
            let output: ContainerLogStream<'static> =
                Box::pin(stream::iter(vec![Ok(LogChunk::stdout(output))]));
            Ok(CommandSession::new(
                CommandExecutionId::new("v7-mongodb-provider"),
                container_id,
                Box::pin(writer),
                output,
            ))
        })
    }

    fn status(&self) -> EngineFuture<'_, CommandStatus> {
        Box::pin(async move {
            while !self.input_complete.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
            Ok(CommandStatus::Exited(0))
        })
    }
}

impl CommandExecutor for RecordingV7MongoDbExecutor {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        self.v8_starts.fetch_add(1, Ordering::AcqRel);
        self.start_session(container.id().clone(), request)
    }

    fn command_status<'operation>(
        &'operation self,
        _execution_id: &'operation CommandExecutionId,
        _container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus> {
        self.status()
    }
}

impl V7ContainerCommandExecutor for RecordingV7MongoDbExecutor {
    fn start_v7_command<'operation>(
        &'operation self,
        target: &'operation V7ContainerCommandTarget,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        self.v7_requests
            .lock()
            .expect("v7 MongoDB requests")
            .push(request.clone());
        self.start_session(target.container_id().clone(), request)
    }

    fn v7_command_status<'operation>(
        &'operation self,
        _execution_id: &'operation CommandExecutionId,
        _container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus> {
        self.status()
    }
}

fn source() -> V7LogicalDataMigrationSource {
    V7LogicalDataMigrationSource::new(V7LogicalDataMigrationSourceOptions {
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "database".to_owned(),
        driver: "mongodb".to_owned(),
        container_name: "bill-database".to_owned(),
        container_id: "legacy-mongodb".to_owned(),
        named_volumes: vec!["bill-database-data".to_owned()],
        logical_data: BTreeMap::from([("database".to_owned(), "legacy_bill".to_owned())]),
    })
    .expect("v7 MongoDB source")
}

fn accepted_inventory() -> AcceptedV7InventoryRecord {
    let source_revision = format!("sha256:{}", "d".repeat(64));
    let inventory_json = serde_json::json!({
        "project_id": "bill",
        "canonical_project_path": "/work/bill",
        "source_revision": source_revision,
        "blockers": [],
        "services": [{
            "service_id": "database",
            "kind": "database",
            "driver": "mongodb",
            "configured_image": "mongo:8.0",
            "container_name": "bill-database",
            "observed_container_id": "legacy-mongodb",
            "configured_mounts": [{
                "source_kind": "named_volume",
                "source": "bill-database-data",
                "target": "/data/db",
                "read_only": false
            }],
            "observed_mounts": [{
                "source_kind": "named_volume",
                "source": "bill-database-data",
                "target": "/data/db",
                "read_only": false
            }],
            "logical_data": {"database": "legacy_bill"}
        }]
    })
    .to_string();
    AcceptedV7InventoryRecord::new(AcceptedV7InventoryRecordOptions {
        project_id: "bill".to_owned(),
        canonical_project_path: PathBuf::from("/work/bill"),
        source_revision,
        inventory_json,
        generated_environment_rollback: None,
        accepted_at_unix_seconds: 62_999,
    })
    .expect("accepted v7 MongoDB inventory")
}

fn target_logical_resource() -> LogicalResourceRecord {
    LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/database".to_owned(),
        shared_resource_id: "mongodb-migration-8".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "mongodb_database".to_owned(),
        compatibility_fingerprint: "sha256:mongodb-8".to_owned(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
}

fn target_credential() -> CredentialRecord {
    CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/mongodb".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "st_bill_database".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    })
}

fn administrator() -> CredentialRecord {
    CredentialRecord::new(CredentialRecordOptions {
        credential_id: "migration/restore-42/mongodb-bootstrap".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "mongodb".to_owned(),
        username: "stackctl_admin".to_owned(),
        secret: "root-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    })
}

fn owned_target_container() -> OwnedContainer {
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::ProjectService,
        project_id: Some("bill".to_owned()),
        compatibility_fingerprint: "sha256:mongodb-8".to_owned(),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("target metadata")
    .with_resource_id("restore-42")
    .expect("target migration identity");
    let observed = ObservedContainer::new(ContainerId::new("mongodb-target"), metadata.labels());

    reconstruct_owned_container(&observed, "install-1", 8).expect("owned MongoDB container")
}

fn backup_root(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!("stackctl-{label}-{unique}"))
}
