use super::{
    RedisBackupOptions, RedisRestoreOptions, V7RedisCredential, V7RedisMigrationProvider,
    V7RedisMigrationProviderOptions, V7RedisSourceRetirement, backup_redis_prefix,
    restore_redis_prefix,
};
use crate::control_plane::engine::{
    CommandExecutionId, CommandExecutor, CommandRequest, CommandSession, CommandStatus,
    ContainerId, ContainerLogStream, EngineFuture, LogChunk, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ObservedContainer, ResourceKind, RetentionClass,
    V7ContainerCommandExecutor, V7ContainerCommandTarget, reconstruct_owned_container,
};
use crate::control_plane::migration::{
    MigrationFuture, V7LogicalDataMigrationSource, V7LogicalDataMigrationSourceOptions,
    V7MigrationAdapterTarget, V7RecoverableMigrationProvider,
};
use crate::control_plane::shared_infrastructure::{CredentialSecret, RedisAclProject, RedisFlavor};
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
fn v7_redis_provider_namespaces_recovery_replays_and_retires_on_confirmation() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("v7 Redis provider runtime");
    let root = backup_root("v7-redis-provider");
    let snapshot = concat!(
        r#"{"format":1,"created_at_unix_seconds":65000,"prefix_hex":"737461636b63746c3a62696c6c3a63616368653a","records":["#,
        r#"{"key_hex":"737461636b63746c3a62696c6c3a63616368653a666f6f","dump_hex":"0001ff","ttl_milliseconds":-1}]}"#,
    );
    let executor = ProviderExecutor::new(snapshot.as_bytes().to_vec());
    let accepted = accepted_inventory();
    let source = v7_source();
    let source_credential =
        V7RedisCredential::new("default", "legacy-secret", 0).expect("source credential");
    let logical = logical_resource();
    let credential = credential();
    let acl = RedisAclProject::new(
        "bill",
        "cache",
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("target ACL");
    let administrator = administrator();
    let container = owned_container();
    let retirement = ProviderRetirement::default();
    let retired = Arc::clone(&retirement.called);
    let options = V7RedisMigrationProviderOptions {
        accepted: &accepted,
        source: &source,
        flavor: RedisFlavor::Redis,
        source_credential: &source_credential,
        target_container: &container,
        target_logical_resource: &logical,
        target_credential: &credential,
        target_acl: &acl,
        administrator: &administrator,
        installation_id: "install-1",
        backup_root: &root,
        created_at_unix_seconds: 65_000,
        verified_at_unix_seconds: 65_001,
        timeout: Duration::from_secs(30),
    };
    let debug = format!("{options:?}");
    assert!(!debug.contains("legacy-secret"));
    assert!(!debug.contains("project-secret"));
    assert!(!debug.contains("admin-secret"));
    let mut provider =
        V7RedisMigrationProvider::new(&executor, retirement, options).expect("v7 Redis provider");

    let backup = runtime
        .block_on(provider.backup_source(&source))
        .expect("backup source keyspace");
    assert_eq!(
        std::fs::read(Path::new(backup.reference()).join("artifact.bin")).expect("backup artifact"),
        snapshot.as_bytes()
    );
    {
        let request = executor.v7_request.lock().expect("v7 request");
        let request = request.as_ref().expect("recorded v7 request");
        assert_eq!(request.arguments()[0], "redis-cli");
        assert!(request.arguments().contains(&"-n".to_owned()));
        assert!(
            request
                .arguments()
                .iter()
                .any(|argument| argument.contains("target_prefix .. source_key"))
        );
    }
    let checkpoint =
        V7MigrationAdapterCheckpoint::pending("service/cache", "redis-tenant-prefix", true, 64_999)
            .expect("pending checkpoint")
            .with_recovery_verified(
                backup.reference(),
                backup.artifact_sha256(),
                backup.artifact_size_bytes(),
                65_001,
            )
            .expect("verified checkpoint");
    let artifact = Path::new(backup.reference()).join("artifact.bin");
    std::fs::write(&artifact, b"tampered").expect("tamper backup");
    let error = runtime
        .block_on(provider.restore_and_verify_target(&source, &checkpoint))
        .expect_err("tampered recovery must block target mutation");
    assert!(error.to_string().contains("checksum does not match"));
    assert_eq!(executor.v8_starts.load(Ordering::Acquire), 0);
    std::fs::write(&artifact, snapshot).expect("restore fixture");

    let expected =
        V7MigrationAdapterTarget::resource("redis:bill/cache/redis:stackctl:bill:cache:")
            .expect("target reference");
    assert_eq!(
        runtime
            .block_on(provider.restore_and_verify_target(&source, &checkpoint))
            .expect("restore target"),
        expected
    );
    assert_eq!(
        runtime
            .block_on(provider.restore_and_verify_target(&source, &checkpoint))
            .expect("replay restore"),
        expected
    );
    assert_eq!(executor.v8_starts.load(Ordering::Acquire), 6);
    runtime
        .block_on(provider.verify_target(&source, "redis:bill/cache/redis:stackctl:bill:cache:"))
        .expect("verify target");
    runtime
        .block_on(provider.verify_source(&source))
        .expect("verify source");
    assert!(!retired.load(Ordering::Acquire));
    runtime
        .block_on(provider.retire_source(&source))
        .expect("retire source");
    assert!(retired.load(Ordering::Acquire));

    drop(provider);
    std::fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn v7_valkey_commands_use_the_native_client_and_bind_acl_secret() {
    let source_credential = V7RedisCredential::new("default", "", 3).expect("source credential");
    let request = super::verify_v7_redis_source::verification_request(
        RedisFlavor::Valkey,
        source_credential.username(),
        source_credential.password(),
        source_credential.database(),
    )
    .expect("Valkey verification request");
    let acl = RedisAclProject::new(
        "bill",
        "cache",
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("target ACL");

    assert_eq!(request.arguments()[0], "valkey-cli");
    assert!(request.environment().is_empty());
    assert!(acl.matches_credential(&credential()));
    let mismatched = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/cache/redis".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "cache".to_owned(),
        username: "st_bill_cache".to_owned(),
        secret: "other-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    assert!(!acl.matches_credential(&mismatched));
}

#[test]
fn redis_backup_streams_one_atomic_binary_safe_prefix_snapshot() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Redis backup runtime");
    let root = std::env::temp_dir().join(format!(
        "stackctl-redis-prefix-backup-{}",
        std::process::id()
    ));
    let snapshot = concat!(
        r#"{"format":1,"created_at_unix_seconds":47000,"prefix_hex":"737461636b63746c3a62696c6c3a63616368653a","records":["#,
        r#"{"key_hex":"737461636b63746c3a62696c6c3a63616368653a666f6f","dump_hex":"0001ff","ttl_milliseconds":-1}]}"#,
    );
    let executor = RecordingExecutor::new(snapshot.as_bytes().to_vec());
    let logical = logical_resource();
    let credential = credential();
    let administrator = administrator();
    let container = owned_container();

    let backup = runtime
        .block_on(backup_redis_prefix(
            &executor,
            &container,
            &RedisBackupOptions {
                flavor: RedisFlavor::Redis,
                logical_resource: &logical,
                credential: &credential,
                administrator: &administrator,
                prefix: "stackctl:bill:cache:",
                installation_id: "install-1",
                created_at_unix_seconds: 47_000,
                backup_root: &root,
                timeout: Duration::from_secs(30),
            },
        ))
        .expect("Redis prefix backup");

    let request = executor
        .request
        .lock()
        .expect("backup request")
        .clone()
        .expect("executed backup request");
    assert_eq!(request.arguments()[0], "redis-cli");
    assert!(request.arguments().contains(&"EVAL".to_owned()));
    assert!(
        request
            .arguments()
            .contains(&"stackctl:bill:cache:".to_owned())
    );
    assert!(
        request
            .arguments()
            .iter()
            .any(|value| value.contains("DUMP"))
    );
    assert!(
        request
            .arguments()
            .iter()
            .any(|value| value.contains("PTTL"))
    );
    assert_eq!(request.environment()["REDISCLI_AUTH"], "admin-secret");
    assert!(!format!("{:?}", request.arguments()).contains("admin-secret"));
    assert_eq!(
        std::fs::read(Path::new(backup.reference()).join("artifact.bin"))
            .expect("stored Redis snapshot"),
        snapshot.as_bytes()
    );

    std::fs::remove_dir_all(root).expect("remove Redis backup fixture");
}

#[test]
fn redis_backup_rejects_cross_prefix_records_before_recovery_publication() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Redis backup runtime");
    let root = std::env::temp_dir().join(format!(
        "stackctl-redis-cross-prefix-backup-{}",
        std::process::id()
    ));
    let snapshot = concat!(
        r#"{"format":1,"created_at_unix_seconds":48000,"prefix_hex":"737461636b63746c3a62696c6c3a63616368653a","records":["#,
        r#"{"key_hex":"737461636b63746c3a73686f703a63616368653a666f6f","dump_hex":"0001ff","ttl_milliseconds":-1}]}"#,
    );
    let executor = RecordingExecutor::new(snapshot.as_bytes().to_vec());
    let logical = logical_resource();
    let credential = credential();
    let administrator = administrator();
    let container = owned_container();

    let error = runtime
        .block_on(backup_redis_prefix(
            &executor,
            &container,
            &RedisBackupOptions {
                flavor: RedisFlavor::Redis,
                logical_resource: &logical,
                credential: &credential,
                administrator: &administrator,
                prefix: "stackctl:bill:cache:",
                installation_id: "install-1",
                created_at_unix_seconds: 48_000,
                backup_root: &root,
                timeout: Duration::from_secs(30),
            },
        ))
        .expect_err("cross-prefix snapshot");

    assert!(
        error
            .to_string()
            .contains("invalid or duplicate prefix records")
    );

    std::fs::remove_dir_all(root).expect("remove invalid Redis backup fixture");
}

#[test]
fn redis_restore_stages_verified_records_and_replaces_only_the_exact_prefix() {
    use crate::control_plane::retention::{
        BackupResourceIdentity, store_backup_artifact_for_identity, verify_stored_backup_artifact,
    };
    use crate::control_plane::state::{RecoveryPointRecord, RecoveryPointRecordOptions};

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Redis restore runtime");
    let root = std::env::temp_dir().join(format!(
        "stackctl-redis-prefix-restore-{}",
        std::process::id()
    ));
    let snapshot = concat!(
        r#"{"format":1,"created_at_unix_seconds":47000,"prefix_hex":"737461636b63746c3a62696c6c3a63616368653a","records":["#,
        r#"{"key_hex":"737461636b63746c3a62696c6c3a63616368653a666f6f","dump_hex":"0001ff","ttl_milliseconds":-1},"#,
        r#"{"key_hex":"737461636b63746c3a62696c6c3a63616368653a626172","dump_hex":"0002ff","ttl_milliseconds":6000},"#,
        r#"{"key_hex":"737461636b63746c3a62696c6c3a63616368653a6f6c64","dump_hex":"0003ff","ttl_milliseconds":1000}]}"#,
    );
    let logical = logical_resource();
    let credential = credential();
    let administrator = administrator();
    let container = owned_container();
    let identity = BackupResourceIdentity::from_logical(&logical, "install-1");
    let stored = store_backup_artifact_for_identity(&identity, snapshot.as_bytes(), 47_000, &root)
        .expect("stored Redis snapshot");
    let evidence = verify_stored_backup_artifact(&stored, 47_001).expect("verified snapshot");
    let recovery = RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "backup-redis".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "cache".to_owned(),
        logical_resource_id: logical.logical_resource_id().to_owned(),
        resource_kind: logical.kind().to_owned(),
        compatibility_fingerprint: logical.compatibility_fingerprint().to_owned(),
        reference: stored.recovery_point().display().to_string(),
        artifact_sha256: evidence.artifact_sha256().to_owned(),
        artifact_size_bytes: evidence.artifact_size_bytes(),
        created_at_unix_seconds: 47_000,
        verified_at_unix_seconds: 47_001,
    })
    .expect("Redis recovery point");
    let executor = RecordingRestoreExecutor::default();

    runtime
        .block_on(restore_redis_prefix(
            &executor,
            &container,
            &RedisRestoreOptions {
                flavor: RedisFlavor::Redis,
                logical_resource: &logical,
                credential: &credential,
                administrator: &administrator,
                recovery_point: &recovery,
                prefix: "stackctl:bill:cache:",
                installation_id: "install-1",
                restored_at_unix_seconds: 47_003,
                timeout: Duration::from_secs(30),
            },
        ))
        .expect("restore Redis prefix");

    let calls = executor.requests.lock().expect("restore requests").clone();
    assert_eq!(calls.len(), 2);
    assert!(
        calls
            .iter()
            .all(|request| request.arguments()[0] == "redis-cli")
    );
    assert!(
        calls
            .iter()
            .all(|request| request.arguments().contains(&"-x".to_owned()))
    );
    assert!(
        calls[0]
            .arguments()
            .iter()
            .any(|value| value.contains("RESTORE"))
    );
    assert!(
        !calls[0]
            .arguments()
            .iter()
            .any(|value| value.contains("MATCH', prefix .. '*'"))
    );
    assert!(
        calls[1]
            .arguments()
            .iter()
            .any(|value| value.contains("UNLINK"))
    );
    assert!(
        calls[1]
            .arguments()
            .iter()
            .any(|value| value.contains("RENAME"))
    );
    assert!(calls.iter().all(|request| {
        request.environment()["REDISCLI_AUTH"] == "admin-secret"
            && !format!("{:?}", request.arguments()).contains("admin-secret")
    }));
    let inputs = executor.inputs.lock().expect("restore inputs").clone();
    assert_eq!(inputs.len(), 2);
    assert_eq!(inputs[0], inputs[1]);
    let payload: serde_json::Value =
        serde_json::from_slice(&inputs[0]).expect("restore payload JSON");
    let records = payload["records"].as_array().expect("restore records");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["ttl_milliseconds"], "3000");
    assert_eq!(records[1]["ttl_milliseconds"], "0");
    assert!(inputs[0].windows(6).all(|window| window != b"0003ff"));

    std::fs::remove_dir_all(root).expect("remove Redis restore fixture");
}

struct ProviderExecutor {
    snapshot: Vec<u8>,
    v7_request: Mutex<Option<CommandRequest>>,
    restored_inputs: Arc<Mutex<Vec<Vec<u8>>>>,
    input_complete: Arc<AtomicBool>,
    v8_starts: AtomicUsize,
}

impl ProviderExecutor {
    fn new(snapshot: Vec<u8>) -> Self {
        Self {
            snapshot,
            v7_request: Mutex::new(None),
            restored_inputs: Arc::new(Mutex::new(Vec::new())),
            input_complete: Arc::new(AtomicBool::new(true)),
            v8_starts: AtomicUsize::new(0),
        }
    }

    fn start_session(
        &self,
        container_id: ContainerId,
        request: &CommandRequest,
        v7: bool,
    ) -> EngineFuture<'_, CommandSession> {
        let exports_snapshot = request
            .arguments()
            .iter()
            .any(|argument| argument.contains("cjson.encode"));
        let captures_restore = request.arguments().iter().any(|argument| argument == "-x");
        let output = if exports_snapshot {
            self.snapshot.clone()
        } else if v7 {
            b"default\nPONG\n".to_vec()
        } else if captures_restore {
            Vec::new()
        } else {
            b"st_bill_cache\nPONG\n".to_vec()
        };
        let restored_inputs = Arc::clone(&self.restored_inputs);
        let input_complete = Arc::clone(&self.input_complete);
        input_complete.store(false, Ordering::Release);

        Box::pin(async move {
            let (writer, mut reader) = duplex(64 * 1024);
            tokio::spawn(async move {
                let mut input = Vec::new();
                reader
                    .read_to_end(&mut input)
                    .await
                    .expect("drain Redis provider command input");
                if captures_restore {
                    restored_inputs
                        .lock()
                        .expect("restored Redis inputs")
                        .push(input);
                }
                input_complete.store(true, Ordering::Release);
            });
            let output: ContainerLogStream<'static> =
                Box::pin(stream::iter(vec![Ok(LogChunk::stdout(output))]));
            Ok(CommandSession::new(
                CommandExecutionId::new("v7-redis-provider"),
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

impl CommandExecutor for ProviderExecutor {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation crate::control_plane::engine::OwnedContainer,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        self.v8_starts.fetch_add(1, Ordering::AcqRel);
        self.start_session(container.id().clone(), request, false)
    }

    fn command_status<'operation>(
        &'operation self,
        _execution_id: &'operation CommandExecutionId,
        _container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus> {
        self.status()
    }
}

impl V7ContainerCommandExecutor for ProviderExecutor {
    fn start_v7_command<'operation>(
        &'operation self,
        target: &'operation V7ContainerCommandTarget,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        *self.v7_request.lock().expect("v7 Redis request") = Some(request.clone());
        self.start_session(target.container_id().clone(), request, true)
    }

    fn v7_command_status<'operation>(
        &'operation self,
        _execution_id: &'operation CommandExecutionId,
        _container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus> {
        self.status()
    }
}

#[derive(Default)]
struct ProviderRetirement {
    called: Arc<AtomicBool>,
}

impl V7RedisSourceRetirement for ProviderRetirement {
    fn retire_source<'operation>(
        &'operation mut self,
        source: &'operation V7LogicalDataMigrationSource,
    ) -> MigrationFuture<'operation, ()> {
        assert_eq!(source, &v7_source());
        self.called.store(true, Ordering::Release);
        Box::pin(async { Ok(()) })
    }
}

fn backup_root(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stackctl-{name}-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create backup root");
    root
}

fn v7_source() -> V7LogicalDataMigrationSource {
    V7LogicalDataMigrationSource::new(V7LogicalDataMigrationSourceOptions {
        project_id: "bill".to_owned(),
        service_id: "cache".to_owned(),
        kind: "cache".to_owned(),
        driver: "redis".to_owned(),
        container_name: "bill-cache".to_owned(),
        container_id: "legacy-redis".to_owned(),
        named_volumes: vec!["bill-cache-data".to_owned()],
        logical_data: BTreeMap::new(),
    })
    .expect("v7 Redis source")
}

fn accepted_inventory() -> AcceptedV7InventoryRecord {
    let source_revision = format!("sha256:{}", "b".repeat(64));
    let inventory_json = serde_json::json!({
        "project_id": "bill",
        "canonical_project_path": "/work/bill",
        "source_revision": source_revision,
        "blockers": [],
        "services": [{
            "service_id": "cache",
            "kind": "cache",
            "driver": "redis",
            "configured_image": "redis:7",
            "container_name": "bill-cache",
            "observed_container_id": "legacy-redis",
            "configured_mounts": [{
                "source_kind": "named_volume",
                "source": "bill-cache-data",
                "target": "/data",
                "read_only": false
            }],
            "observed_mounts": [{
                "source_kind": "named_volume",
                "source": "bill-cache-data",
                "target": "/data",
                "read_only": false
            }],
            "logical_data": {}
        }]
    })
    .to_string();
    AcceptedV7InventoryRecord::new(AcceptedV7InventoryRecordOptions {
        project_id: "bill".to_owned(),
        canonical_project_path: PathBuf::from("/work/bill"),
        source_revision,
        inventory_json,
        generated_environment_rollback: None,
        accepted_at_unix_seconds: 64_999,
    })
    .expect("accepted v7 Redis inventory")
}

#[derive(Clone, Default)]
struct RecordingRestoreExecutor {
    requests: Arc<Mutex<Vec<CommandRequest>>>,
    inputs: Arc<Mutex<Vec<Vec<u8>>>>,
    completed: Arc<AtomicUsize>,
}

impl CommandExecutor for RecordingRestoreExecutor {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation crate::control_plane::engine::OwnedContainer,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        let index = self.requests.lock().expect("restore requests").len();
        self.requests
            .lock()
            .expect("restore requests")
            .push(request.clone());
        let inputs = Arc::clone(&self.inputs);
        let completed = Arc::clone(&self.completed);
        let container_id = container.id().clone();
        Box::pin(async move {
            let (writer, mut reader) = duplex(16 * 1024);
            tokio::spawn(async move {
                let mut input = Vec::new();
                reader
                    .read_to_end(&mut input)
                    .await
                    .expect("drain Redis restore input");
                inputs.lock().expect("restore inputs").push(input);
                completed.store(index + 1, Ordering::Release);
            });
            Ok(CommandSession::new(
                CommandExecutionId::new(format!("redis-restore-{index}")),
                container_id,
                Box::pin(writer),
                Box::pin(stream::empty()),
            ))
        })
    }

    fn command_status<'operation>(
        &'operation self,
        execution_id: &'operation CommandExecutionId,
        _container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus> {
        let index = execution_id
            .as_str()
            .strip_prefix("redis-restore-")
            .and_then(|value| value.parse::<usize>().ok())
            .expect("restore execution index");
        Box::pin(async move {
            while self.completed.load(Ordering::Acquire) <= index {
                tokio::task::yield_now().await;
            }
            Ok(CommandStatus::Exited(0))
        })
    }
}

struct RecordingExecutor {
    request: Mutex<Option<CommandRequest>>,
    input_complete: Arc<AtomicBool>,
    output: Vec<u8>,
}

impl RecordingExecutor {
    fn new(output: Vec<u8>) -> Self {
        Self {
            request: Mutex::new(None),
            input_complete: Arc::new(AtomicBool::new(false)),
            output,
        }
    }
}

impl CommandExecutor for RecordingExecutor {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation crate::control_plane::engine::OwnedContainer,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        *self.request.lock().expect("request lock") = Some(request.clone());
        let input_complete = Arc::clone(&self.input_complete);
        let output = self.output.clone();
        let container_id = container.id().clone();
        Box::pin(async move {
            let (writer, mut reader) = duplex(1_024);
            tokio::spawn(async move {
                let mut input = Vec::new();
                reader
                    .read_to_end(&mut input)
                    .await
                    .expect("drain Redis command input");
                input_complete.store(true, Ordering::Release);
            });
            let output: ContainerLogStream<'static> =
                Box::pin(stream::iter(vec![Ok(LogChunk::stdout(output))]));
            Ok(CommandSession::new(
                CommandExecutionId::new("redis-backup"),
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
        Box::pin(async move {
            while !self.input_complete.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
            Ok(CommandStatus::Exited(0))
        })
    }
}

fn logical_resource() -> LogicalResourceRecord {
    LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/cache/redis".to_owned(),
        shared_resource_id: "redis-shared".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "cache".to_owned(),
        kind: "redis_acl_prefix".to_owned(),
        compatibility_fingerprint: format!("sha256:{}", "a".repeat(64)),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
}

fn credential() -> CredentialRecord {
    CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/cache/redis".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "cache".to_owned(),
        username: "st_bill_cache".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    })
}

fn administrator() -> CredentialRecord {
    CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("shared/{}/redis-bootstrap", "a".repeat(64)),
        project_id: None,
        service_id: "redis".to_owned(),
        username: "stackctl_admin".to_owned(),
        secret: "admin-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    })
}

fn owned_container() -> crate::control_plane::engine::OwnedContainer {
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::SharedService,
        project_id: None,
        compatibility_fingerprint: format!("sha256:{}", "a".repeat(64)),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("Redis metadata");
    let observed = ObservedContainer::new(ContainerId::new("redis-source"), metadata.labels());

    reconstruct_owned_container(&observed, "install-1", 8).expect("owned Redis container")
}
