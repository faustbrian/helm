use super::{RedisBackupOptions, backup_redis_prefix};
use crate::control_plane::engine::{
    CommandExecutionId, CommandExecutor, CommandRequest, CommandSession, CommandStatus,
    ContainerId, ContainerLogStream, EngineFuture, LogChunk, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ObservedContainer, ResourceKind, RetentionClass,
    reconstruct_owned_container,
};
use crate::control_plane::shared_infrastructure::RedisFlavor;
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
    LogicalResourceRecordOptions, ResourceLifecycle,
};
use futures_util::stream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, duplex};

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
        std::fs::read(std::path::Path::new(backup.reference()).join("artifact.bin"))
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
