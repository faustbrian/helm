use super::{
    EngineV7SqlServerSourceRetirement, V7SqlServerCredential, V7SqlServerMigrationProvider,
    V7SqlServerMigrationProviderOptions, V7SqlServerSourceRetirement,
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
use crate::control_plane::shared_infrastructure::{CredentialSecret, SqlServerLogicalResourcePlan};
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
fn v7_sql_server_provider_is_recovery_bound_replayable_and_confirmed() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("v7 SQL Server runtime");
    let root = backup_root();
    let dump = b"native sql server backup".to_vec();
    let executor = RecordingExecutor::new(dump.clone());
    let accepted = accepted_inventory();
    let source = source();
    let source_credential =
        V7SqlServerCredential::new("sa", "LegacyPass1").expect("source credential");
    let target = target_logical_resource();
    let target_credential = target_credential();
    let target_plan = SqlServerLogicalResourcePlan::new(
        "bill",
        "database",
        CredentialSecret::new("ProjectPass1".to_owned()),
    )
    .expect("target plan");
    let administrator = administrator();
    let target_container = owned_target_container();
    let mut retirement = RecordingRetirement::default();
    let retired = Arc::clone(&retirement.called);
    let options = V7SqlServerMigrationProviderOptions {
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
        created_at_unix_seconds: 64_000,
        verified_at_unix_seconds: 64_001,
        timeout: Duration::from_secs(30),
    };
    let debug = format!("{options:?}");
    assert!(!debug.contains("LegacyPass1"));
    assert!(!debug.contains("ProjectPass1"));
    assert!(!debug.contains("RootPass1"));
    let mut provider = V7SqlServerMigrationProvider::new(&executor, &mut retirement, options)
        .expect("v7 SQL Server provider");

    let backup = runtime
        .block_on(provider.backup_source(&source))
        .expect("backup source");
    assert_eq!(
        std::fs::read(Path::new(backup.reference()).join("artifact.bin")).expect("backup artifact"),
        dump
    );
    let checkpoint = V7MigrationAdapterCheckpoint::pending(
        "service/database",
        "sqlserver-logical-database",
        true,
        63_999,
    )
    .expect("pending checkpoint")
    .with_recovery_verified(
        backup.reference(),
        backup.artifact_sha256(),
        backup.artifact_size_bytes(),
        64_001,
    )
    .expect("verified checkpoint");

    let artifact = Path::new(backup.reference()).join("artifact.bin");
    std::fs::write(&artifact, b"tampered").expect("tamper backup");
    let error = runtime
        .block_on(provider.restore_and_verify_target(&source, &checkpoint))
        .expect_err("tampering must block mutation");
    assert!(error.to_string().contains("checksum does not match"));
    assert_eq!(executor.v8_starts.load(Ordering::Acquire), 0);
    std::fs::write(&artifact, &dump).expect("restore backup fixture");

    let expected =
        V7MigrationAdapterTarget::resource("sqlserver:bill/database:stackctl_bill_database")
            .expect("target reference");
    assert_eq!(
        runtime
            .block_on(provider.restore_and_verify_target(&source, &checkpoint))
            .expect("prepare target"),
        expected
    );
    assert_eq!(
        runtime
            .block_on(provider.restore_and_verify_target(&source, &checkpoint))
            .expect("replay target"),
        expected
    );
    assert_eq!(executor.v8_starts.load(Ordering::Acquire), 4);
    runtime
        .block_on(provider.verify_target(&source, "sqlserver:bill/database:stackctl_bill_database"))
        .expect("verify target");
    runtime
        .block_on(provider.verify_source(&source))
        .expect("verify retained source");
    assert!(!retired.load(Ordering::Acquire));
    runtime
        .block_on(provider.retire_source(&source))
        .expect("retire source");
    assert!(retired.load(Ordering::Acquire));

    {
        let request = executor.restore_request.lock().expect("restore request");
        let request = request.as_ref().expect("recorded restore request");
        assert!(!format!("{request:?}").contains("RootPass1"));
        assert!(
            request
                .environment()
                .get("STACKCTL_RESET_SQL")
                .is_some_and(|sql| sql.contains("DROP DATABASE [stackctl_bill_database]"))
        );
        assert!(
            request
                .environment()
                .get("STACKCTL_RESTORE_SQL")
                .is_some_and(|sql| sql.contains("RESTORE DATABASE [stackctl_bill_database]"))
        );
    }
    assert_eq!(
        executor
            .restored_input
            .lock()
            .expect("restored input")
            .as_slice(),
        dump
    );
    drop(provider);
    std::fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn sql_server_target_plan_binds_the_exact_durable_secret() {
    let plan = SqlServerLogicalResourcePlan::new(
        "bill",
        "database",
        CredentialSecret::new("ProjectPass1".to_owned()),
    )
    .expect("target plan");
    let matching = target_credential();
    let mismatched = CredentialRecord::new(CredentialRecordOptions {
        credential_id: matching.credential_id().to_owned(),
        project_id: matching.project_id().map(str::to_owned),
        service_id: matching.service_id().to_owned(),
        username: matching.username().to_owned(),
        secret: "OtherPass1".to_owned(),
        lifecycle: matching.lifecycle(),
    });
    assert!(plan.matches_credential(&matching));
    assert!(!plan.matches_credential(&mismatched));
}

#[test]
fn engine_v7_sql_server_retirement_forwards_exact_resources() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("retirement runtime");
    let source = source();
    let mut engine = RecordingContainerRetirement::default();
    let mut retirement = EngineV7SqlServerSourceRetirement::new(&mut engine);
    runtime
        .block_on(retirement.retire_source(&source))
        .expect("retire source");
    assert_eq!(
        engine
            .target
            .into_inner()
            .expect("retirement target")
            .expect("recorded target")
            .named_volumes(),
        ["bill-database-data"]
    );
}

#[derive(Default)]
struct RecordingRetirement {
    called: Arc<AtomicBool>,
}

impl V7SqlServerSourceRetirement for RecordingRetirement {
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
        &'operation self,
        target: &'operation V7ContainerRetirementTarget,
    ) -> EngineFuture<'operation, ()> {
        *self.target.lock().expect("retirement target") = Some(target.clone());
        Box::pin(async { Ok(()) })
    }
}

struct RecordingExecutor {
    dump: Vec<u8>,
    restore_request: Mutex<Option<CommandRequest>>,
    restored_input: Arc<Mutex<Vec<u8>>>,
    input_complete: Arc<AtomicBool>,
    v8_starts: AtomicUsize,
}

impl RecordingExecutor {
    fn new(dump: Vec<u8>) -> Self {
        Self {
            dump,
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
        let database = request
            .arguments()
            .windows(2)
            .find_map(|pair| (pair[0] == "-d").then_some(pair[1].as_str()));
        let output = if environment.contains_key("STACKCTL_BACKUP_SQL") {
            self.dump.clone()
        } else if database == Some("legacy_bill") {
            b"legacy_bill\tsa\n".to_vec()
        } else if database == Some("stackctl_bill_database") {
            b"stackctl_bill_database\tst_bill_database\n".to_vec()
        } else {
            Vec::new()
        };
        let capture = environment.contains_key("STACKCTL_RESTORE_SQL");
        if capture {
            *self.restore_request.lock().expect("restore request") = Some(request.clone());
        }
        let restored_input = Arc::clone(&self.restored_input);
        let input_complete = Arc::clone(&self.input_complete);
        input_complete.store(false, Ordering::Release);
        Box::pin(async move {
            let (writer, mut reader) = duplex(64 * 1024);
            tokio::spawn(async move {
                let mut input = Vec::new();
                reader.read_to_end(&mut input).await.expect("drain input");
                if capture {
                    *restored_input.lock().expect("restored input") = input;
                }
                input_complete.store(true, Ordering::Release);
            });
            let output: ContainerLogStream<'static> =
                Box::pin(stream::iter(vec![Ok(LogChunk::stdout(output))]));
            Ok(CommandSession::new(
                CommandExecutionId::new("v7-sqlserver-provider"),
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

impl CommandExecutor for RecordingExecutor {
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

impl V7ContainerCommandExecutor for RecordingExecutor {
    fn start_v7_command<'operation>(
        &'operation self,
        target: &'operation V7ContainerCommandTarget,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
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
        driver: "sqlserver".to_owned(),
        container_name: "bill-database".to_owned(),
        container_id: "legacy-sqlserver".to_owned(),
        named_volumes: vec!["bill-database-data".to_owned()],
        logical_data: BTreeMap::from([("database".to_owned(), "legacy_bill".to_owned())]),
    })
    .expect("source")
}

fn accepted_inventory() -> AcceptedV7InventoryRecord {
    let source_revision = format!("sha256:{}", "e".repeat(64));
    let inventory_json = serde_json::json!({
        "project_id": "bill",
        "canonical_project_path": "/work/bill",
        "source_revision": source_revision,
        "blockers": [],
        "services": [{
            "service_id": "database",
            "kind": "database",
            "driver": "sqlserver",
            "configured_image": "mcr.microsoft.com/mssql/server:2022-latest",
            "container_name": "bill-database",
            "observed_container_id": "legacy-sqlserver",
            "configured_mounts": [{"source_kind":"named_volume","source":"bill-database-data","target":"/var/opt/mssql","read_only":false}],
            "observed_mounts": [{"source_kind":"named_volume","source":"bill-database-data","target":"/var/opt/mssql","read_only":false}],
            "logical_data": {"database":"legacy_bill"}
        }]
    })
    .to_string();
    AcceptedV7InventoryRecord::new(AcceptedV7InventoryRecordOptions {
        project_id: "bill".to_owned(),
        canonical_project_path: PathBuf::from("/work/bill"),
        source_revision,
        inventory_json,
        generated_environment_rollback: None,
        accepted_at_unix_seconds: 63_999,
    })
    .expect("accepted inventory")
}

fn target_logical_resource() -> LogicalResourceRecord {
    LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/database".to_owned(),
        shared_resource_id: "sqlserver-migration".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "sqlserver_database".to_owned(),
        compatibility_fingerprint: "sha256:sqlserver-2022".to_owned(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
}

fn target_credential() -> CredentialRecord {
    CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/sqlserver".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "st_bill_database".to_owned(),
        secret: "ProjectPass1".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    })
}

fn administrator() -> CredentialRecord {
    CredentialRecord::new(CredentialRecordOptions {
        credential_id: "migration/restore-42/sqlserver-bootstrap".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "sqlserver".to_owned(),
        username: "sa".to_owned(),
        secret: "RootPass1".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    })
}

fn owned_target_container() -> OwnedContainer {
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::ProjectService,
        project_id: Some("bill".to_owned()),
        compatibility_fingerprint: "sha256:sqlserver-2022".to_owned(),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("target metadata")
    .with_resource_id("restore-42")
    .expect("target identity");
    reconstruct_owned_container(
        &ObservedContainer::new(ContainerId::new("sqlserver-target"), metadata.labels()),
        "install-1",
        8,
    )
    .expect("owned target")
}

fn backup_root() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!("stackctl-v7-sqlserver-{unique}"))
}
