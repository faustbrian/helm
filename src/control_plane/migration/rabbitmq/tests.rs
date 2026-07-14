use super::{
    V7RabbitMqCredential, V7RabbitMqMigrationProvider, V7RabbitMqMigrationProviderOptions,
    V7RabbitMqSourceRetirement, backup_v7_rabbitmq_vhost::validate_no_messages,
    transform_v7_rabbitmq_definitions,
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
use crate::control_plane::shared_infrastructure::{
    CredentialSecret, RabbitMqPasswordHash, RabbitMqProjectDefinition,
};
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
fn v7_rabbitmq_provider_remaps_replays_and_retires_only_after_confirmation() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("v7 RabbitMQ provider runtime");
    let root = backup_root("v7-rabbitmq-provider");
    let source_credential =
        V7RabbitMqCredential::new("legacy-user", "legacy-secret").expect("source credential");
    let source_definitions = source_definitions(&source_credential);
    let executor = ProviderExecutor::new(source_definitions);
    let accepted = accepted_inventory();
    let source = v7_source();
    let logical = logical_resource();
    let credential = credential();
    let definition = RabbitMqProjectDefinition::new(
        "bill",
        "broker",
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("target definition");
    let container = owned_container();
    let mut retirement = ProviderRetirement::default();
    let retired = Arc::clone(&retirement.called);
    let options = V7RabbitMqMigrationProviderOptions {
        accepted: &accepted,
        source: &source,
        source_credential: &source_credential,
        target_container: &container,
        target_logical_resource: &logical,
        target_credential: &credential,
        target_definition: &definition,
        installation_id: "install-1",
        backup_root: &root,
        created_at_unix_seconds: 67_000,
        verified_at_unix_seconds: 67_001,
        timeout: Duration::from_secs(30),
    };
    let debug = format!("{options:?}");
    assert!(!debug.contains("legacy-secret"));
    assert!(!debug.contains("project-secret"));
    let mut provider = V7RabbitMqMigrationProvider::new(&executor, &mut retirement, options)
        .expect("v7 RabbitMQ provider");

    let backup = runtime
        .block_on(provider.backup_source(&source))
        .expect("backup source vhost");
    let archive =
        std::fs::read(Path::new(backup.reference()).join("artifact.bin")).expect("backup artifact");
    let archive_json: serde_json::Value =
        serde_json::from_slice(&archive).expect("transformed archive");
    assert_eq!(archive_json["users"][0]["name"], "st_bill_broker");
    assert_eq!(archive_json["vhosts"][0]["name"], "stackctl_bill_broker");
    assert_eq!(archive_json["queues"][0]["vhost"], "stackctl_bill_broker");
    assert!(!String::from_utf8_lossy(&archive).contains("legacy-user"));
    {
        let requests = executor.v7_requests.lock().expect("v7 requests");
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].arguments()[1], "list_queues");
        assert!(requests[1].arguments()[2].contains("export_definitions"));
        assert!(!format!("{requests:?}").contains("legacy-secret"));
    }
    let checkpoint =
        V7MigrationAdapterCheckpoint::pending("service/broker", "rabbitmq-vhost", true, 66_999)
            .expect("pending checkpoint")
            .with_recovery_verified(
                backup.reference(),
                backup.artifact_sha256(),
                backup.artifact_size_bytes(),
                67_001,
            )
            .expect("verified checkpoint");
    let artifact = Path::new(backup.reference()).join("artifact.bin");
    std::fs::write(&artifact, b"tampered").expect("tamper backup");
    let error = runtime
        .block_on(provider.restore_and_verify_target(&source, &checkpoint))
        .expect_err("tampered recovery must block target mutation");
    assert!(error.to_string().contains("checksum does not match"));
    assert_eq!(executor.v8_starts.load(Ordering::Acquire), 0);
    std::fs::write(&artifact, &archive).expect("restore fixture");

    let expected =
        V7MigrationAdapterTarget::resource("rabbitmq:bill/broker/rabbitmq:stackctl_bill_broker")
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
    assert_eq!(executor.v8_starts.load(Ordering::Acquire), 10);
    let restored = executor.restored_inputs.lock().expect("restored inputs");
    assert_eq!(
        restored.as_slice(),
        [archive.as_slice(), archive.as_slice()]
    );
    drop(restored);
    runtime
        .block_on(provider.verify_target(
            &source,
            "rabbitmq:bill/broker/rabbitmq:stackctl_bill_broker",
        ))
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
fn v7_rabbitmq_definitions_remap_only_the_accepted_topology() {
    let source_credential =
        V7RabbitMqCredential::new("legacy-user", "legacy-secret").expect("source credential");
    let source_hash = RabbitMqPasswordHash::from_salt(
        CredentialSecret::new("legacy-secret".to_owned()),
        [1, 2, 3, 4],
    );
    let target = RabbitMqProjectDefinition::new(
        "bill",
        "broker",
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("target definition");
    let source = serde_json::json!({
        "rabbit_version": "4.1.0",
        "users": [{
            "name": "legacy-user",
            "password_hash": source_hash.encoded(),
            "hashing_algorithm": "rabbit_password_hashing_sha256",
            "tags": ["administrator"]
        }],
        "vhosts": [{"name": "/"}],
        "permissions": [{
            "user": "legacy-user",
            "vhost": "/",
            "configure": ".*",
            "write": ".*",
            "read": ".*"
        }],
        "topic_permissions": [],
        "parameters": [{"vhost": "/", "component": "shovel", "name": "events", "value": {}}],
        "global_parameters": [],
        "policies": [{"vhost": "/", "name": "ha", "pattern": ".*", "definition": {}}],
        "operator_policies": [],
        "queues": [{"vhost": "/", "name": "events", "durable": true, "auto_delete": false, "arguments": {}}],
        "exchanges": [{"vhost": "/", "name": "events", "type": "topic", "durable": true, "auto_delete": false, "internal": false, "arguments": {}}],
        "bindings": [{"vhost": "/", "source": "events", "destination": "events", "destination_type": "queue", "routing_key": "#", "arguments": {}}]
    })
    .to_string();

    let transformed =
        transform_v7_rabbitmq_definitions(source.as_bytes(), "/", &source_credential, &target)
            .expect("transformed definitions");
    let transformed: serde_json::Value = serde_json::from_slice(&transformed).expect("target JSON");

    assert_eq!(transformed["users"][0]["name"], "st_bill_broker");
    assert_eq!(transformed["vhosts"][0]["name"], "stackctl_bill_broker");
    assert_eq!(transformed["queues"][0]["vhost"], "stackctl_bill_broker");
    assert_eq!(transformed["exchanges"][0]["vhost"], "stackctl_bill_broker");
    assert_eq!(transformed["bindings"][0]["vhost"], "stackctl_bill_broker");
    assert!(
        !String::from_utf8_lossy(&serde_json::to_vec(&transformed).expect("encoded target"))
            .contains("legacy-user")
    );
    assert!(!format!("{source_credential:?}").contains("legacy-secret"));
}

#[test]
fn v7_rabbitmq_message_inventory_fails_closed() {
    let error = validate_no_messages("/", b"events\t2\n")
        .expect_err("message-bearing queue must block migration");

    assert!(
        error
            .to_string()
            .contains("cannot preserve message contents")
    );
}

struct ProviderExecutor {
    source_definitions: Vec<u8>,
    v7_requests: Mutex<Vec<CommandRequest>>,
    restored_inputs: Arc<Mutex<Vec<Vec<u8>>>>,
    input_complete: Arc<AtomicBool>,
    v8_starts: AtomicUsize,
}

impl ProviderExecutor {
    fn new(source_definitions: Vec<u8>) -> Self {
        Self {
            source_definitions,
            v7_requests: Mutex::new(Vec::new()),
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
        let command = request
            .arguments()
            .first()
            .map(String::as_str)
            .unwrap_or_default();
        let subcommand = request
            .arguments()
            .get(1)
            .map(String::as_str)
            .unwrap_or_default();
        let script = request
            .arguments()
            .get(2)
            .map(String::as_str)
            .unwrap_or_default();
        let captures_restore = !v7 && script.contains("import_definitions");
        let output = if command == "rabbitmqctl" && subcommand == "list_queues" {
            Vec::new()
        } else if command == "rabbitmqctl" && subcommand == "list_vhosts" {
            b"stackctl_bill_broker\n".to_vec()
        } else if script.contains("export_definitions") {
            if v7 {
                self.source_definitions.clone()
            } else {
                self.restored_inputs
                    .lock()
                    .expect("restored definitions")
                    .last()
                    .cloned()
                    .unwrap_or_default()
            }
        } else {
            Vec::new()
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
                    .expect("drain RabbitMQ provider command input");
                if captures_restore {
                    restored_inputs
                        .lock()
                        .expect("restored RabbitMQ inputs")
                        .push(input);
                }
                input_complete.store(true, Ordering::Release);
            });
            let output: ContainerLogStream<'static> =
                Box::pin(stream::iter(vec![Ok(LogChunk::stdout(output))]));
            Ok(CommandSession::new(
                CommandExecutionId::new("v7-rabbitmq-provider"),
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
        self.v7_requests
            .lock()
            .expect("v7 RabbitMQ requests")
            .push(request.clone());
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

impl V7RabbitMqSourceRetirement for ProviderRetirement {
    fn retire_source<'operation>(
        &'operation mut self,
        source: &'operation V7LogicalDataMigrationSource,
    ) -> MigrationFuture<'operation, ()> {
        assert_eq!(source, &v7_source());
        self.called.store(true, Ordering::Release);
        Box::pin(async { Ok(()) })
    }
}

fn source_definitions(_credential: &V7RabbitMqCredential) -> Vec<u8> {
    let password_hash = RabbitMqPasswordHash::from_salt(
        CredentialSecret::new("legacy-secret".to_owned()),
        [5, 6, 7, 8],
    );
    serde_json::to_vec(&serde_json::json!({
        "rabbit_version": "4.1.0",
        "users": [{
            "name": "legacy-user",
            "password_hash": password_hash.encoded(),
            "hashing_algorithm": "rabbit_password_hashing_sha256",
            "tags": ["administrator"]
        }],
        "vhosts": [{"name": "/"}],
        "permissions": [{
            "user": "legacy-user",
            "vhost": "/",
            "configure": ".*",
            "write": ".*",
            "read": ".*"
        }],
        "topic_permissions": [],
        "parameters": [],
        "global_parameters": [],
        "policies": [],
        "operator_policies": [],
        "queues": [{
            "vhost": "/",
            "name": "events",
            "durable": true,
            "auto_delete": false,
            "arguments": {}
        }],
        "exchanges": [{
            "vhost": "/",
            "name": "events",
            "type": "topic",
            "durable": true,
            "auto_delete": false,
            "internal": false,
            "arguments": {}
        }],
        "bindings": [{
            "vhost": "/",
            "source": "events",
            "destination": "events",
            "destination_type": "queue",
            "routing_key": "#",
            "arguments": {}
        }]
    }))
    .expect("source definitions")
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
        service_id: "broker".to_owned(),
        kind: "app".to_owned(),
        driver: "rabbitmq".to_owned(),
        container_name: "bill-broker".to_owned(),
        container_id: "legacy-rabbitmq".to_owned(),
        named_volumes: vec!["bill-broker-data".to_owned()],
        logical_data: BTreeMap::new(),
    })
    .expect("v7 RabbitMQ source")
}

fn accepted_inventory() -> AcceptedV7InventoryRecord {
    let source_revision = format!("sha256:{}", "d".repeat(64));
    let inventory_json = serde_json::json!({
        "project_id": "bill",
        "canonical_project_path": "/work/bill",
        "source_revision": source_revision,
        "blockers": [],
        "services": [{
            "service_id": "broker",
            "kind": "app",
            "driver": "rabbitmq",
            "configured_image": "rabbitmq:3-management",
            "container_name": "bill-broker",
            "observed_container_id": "legacy-rabbitmq",
            "configured_mounts": [{
                "source_kind": "named_volume",
                "source": "bill-broker-data",
                "target": "/var/lib/rabbitmq",
                "read_only": false
            }],
            "observed_mounts": [{
                "source_kind": "named_volume",
                "source": "bill-broker-data",
                "target": "/var/lib/rabbitmq",
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
        accepted_at_unix_seconds: 66_999,
    })
    .expect("accepted v7 RabbitMQ inventory")
}

fn logical_resource() -> LogicalResourceRecord {
    LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/broker/rabbitmq".to_owned(),
        shared_resource_id: "rabbitmq-shared".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "broker".to_owned(),
        kind: "rabbitmq_vhost_user".to_owned(),
        compatibility_fingerprint: "sha256:rabbitmq-4".to_owned(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
}

fn credential() -> CredentialRecord {
    CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/broker/rabbitmq".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "broker".to_owned(),
        username: "st_bill_broker".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    })
}

fn owned_container() -> crate::control_plane::engine::OwnedContainer {
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::SharedService,
        project_id: None,
        compatibility_fingerprint: "sha256:rabbitmq-4".to_owned(),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("RabbitMQ metadata");
    let observed = ObservedContainer::new(ContainerId::new("rabbitmq-target"), metadata.labels());

    reconstruct_owned_container(&observed, "install-1", 8).expect("owned RabbitMQ container")
}
