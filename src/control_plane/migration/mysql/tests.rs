use super::{
    EngineV7MySqlSourceRetirement, V7MySqlCredential, V7MySqlMigrationProvider,
    V7MySqlMigrationProviderOptions, V7MySqlSourceRetirement,
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
use crate::control_plane::retention::BackupResourceIdentity;
use crate::control_plane::shared_infrastructure::{
    CredentialSecret, MySqlFlavor, MySqlLogicalResourcePlan,
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
fn v7_mysql_provider_is_recovery_bound_replay_safe_and_confirmed() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("v7 MySQL provider runtime");
    let root = backup_root("v7-mysql-provider");
    let dump = b"CREATE TABLE invoices (id BIGINT);".to_vec();
    let executor = RecordingV7MySqlExecutor::new(dump.clone());
    let accepted = accepted_inventory();
    let source = source();
    let source_credential =
        V7MySqlCredential::new("laravel", "legacy-secret").expect("v7 MySQL credential");
    let target = target_logical_resource();
    let target_credential = target_credential();
    let target_plan = MySqlLogicalResourcePlan::new(
        MySqlFlavor::MySql,
        "bill",
        "database",
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("MySQL target plan");
    let administrator = administrator();
    let target_container = owned_target_container();
    let mut retirement = RecordingRetirement::default();
    let retired = Arc::clone(&retirement.called);
    let options = V7MySqlMigrationProviderOptions {
        accepted: &accepted,
        source: &source,
        flavor: MySqlFlavor::MySql,
        source_credential: &source_credential,
        target_container: &target_container,
        target_logical_resource: &target,
        target_credential: &target_credential,
        target_plan: &target_plan,
        administrator: &administrator,
        installation_id: "install-1",
        backup_root: &root,
        created_at_unix_seconds: 61_000,
        verified_at_unix_seconds: 61_001,
        timeout: Duration::from_secs(30),
    };
    let debug = format!("{options:?}");
    assert!(!debug.contains("legacy-secret"));
    assert!(!debug.contains("project-secret"));
    assert!(!debug.contains("root-secret"));
    let mut provider = V7MySqlMigrationProvider::new(&executor, &mut retirement, options)
        .expect("v7 MySQL provider");

    let backup = runtime
        .block_on(provider.backup_source(&source))
        .expect("backup accepted v7 MySQL source");
    assert_eq!(
        std::fs::read(Path::new(backup.reference()).join("artifact.bin"))
            .expect("v7 MySQL backup artifact"),
        dump
    );
    let checkpoint = V7MigrationAdapterCheckpoint::pending(
        "service/database",
        "mysql-logical-database",
        true,
        60_999,
    )
    .expect("pending v7 MySQL checkpoint")
    .with_recovery_verified(
        backup.reference(),
        backup.artifact_sha256(),
        backup.artifact_size_bytes(),
        61_001,
    )
    .expect("recovery-verified v7 MySQL checkpoint");

    let artifact = Path::new(backup.reference()).join("artifact.bin");
    std::fs::write(&artifact, b"tampered").expect("tamper v7 MySQL backup");
    let error = runtime
        .block_on(provider.restore_and_verify_target(&source, &checkpoint))
        .expect_err("tampered recovery must block target mutation");
    assert!(error.to_string().contains("checksum does not match"));
    assert_eq!(executor.v8_starts.load(Ordering::Acquire), 0);
    std::fs::write(&artifact, &dump).expect("restore v7 MySQL backup fixture");

    let expected = V7MigrationAdapterTarget::resource("mysql:bill/database:stackctl_bill_database")
        .expect("expected MySQL target");
    let prepared = runtime
        .block_on(provider.restore_and_verify_target(&source, &checkpoint))
        .expect("prepare v8 MySQL target");
    assert_eq!(prepared, expected);
    let replayed = runtime
        .block_on(provider.restore_and_verify_target(&source, &checkpoint))
        .expect("replay v8 MySQL target preparation");
    assert_eq!(replayed, expected);
    assert_eq!(executor.v8_starts.load(Ordering::Acquire), 6);
    runtime
        .block_on(provider.verify_target(&source, "mysql:bill/database:stackctl_bill_database"))
        .expect("reverify v8 MySQL target");
    runtime
        .block_on(provider.verify_source(&source))
        .expect("verify retained v7 MySQL source");
    assert!(!retired.load(Ordering::Acquire));
    runtime
        .block_on(provider.retire_source(&source))
        .expect("retire confirmed v7 MySQL source");
    assert!(retired.load(Ordering::Acquire));

    let v7_requests = executor.v7_requests.lock().expect("v7 MySQL requests");
    let dump_arguments = v7_requests[0].arguments();
    assert_eq!(
        dump_arguments.first().map(String::as_str),
        Some("mysqldump")
    );
    assert_eq!(
        dump_arguments.last().map(String::as_str),
        Some("legacy_bill")
    );
    assert!(
        !dump_arguments
            .iter()
            .any(|argument| argument == "--databases")
    );
    assert_eq!(
        executor
            .restored_input
            .lock()
            .expect("restored MySQL input")
            .as_slice(),
        dump
    );
    drop(v7_requests);
    drop(provider);
    std::fs::remove_dir_all(root).expect("remove v7 MySQL fixture");
}

#[test]
fn v7_mariadb_commands_use_native_clients_without_embedding_source_schema() {
    let dump = super::backup_v7_mysql_database::dump_arguments(
        MySqlFlavor::MariaDb,
        "laravel",
        "legacy_bill",
    );
    let verify = super::verify_v7_mysql_source::verification_request(
        MySqlFlavor::MariaDb,
        "laravel",
        "secret",
        "legacy_bill",
    )
    .expect("MariaDB verification request");

    assert_eq!(dump.first().map(String::as_str), Some("mariadb-dump"));
    assert_eq!(dump.last().map(String::as_str), Some("legacy_bill"));
    assert!(!dump.iter().any(|argument| argument == "--databases"));
    assert_eq!(
        verify.arguments().first().map(String::as_str),
        Some("mariadb")
    );
}

#[cfg(unix)]
#[test]
fn v7_mysql_backup_rejects_explicit_definers_and_removes_recovery() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("v7 MySQL definer runtime");
    let root = backup_root("v7-mysql-definer");
    let executor =
        RecordingV7MySqlExecutor::new(b"/*!50017 DEFINER=`legacy`@`%`*/ TRIGGER unsafe;".to_vec());
    let target = source().command_target().expect("v7 MySQL command target");
    let credential =
        V7MySqlCredential::new("laravel", "legacy-secret").expect("v7 MySQL credential");
    let identity =
        BackupResourceIdentity::for_v7_logical_data("bill", "database", "mysql", &"c".repeat(64));

    let error = runtime
        .block_on(super::backup_v7_mysql_database(
            &executor,
            &target,
            MySqlFlavor::MySql,
            &credential,
            "legacy_bill",
            &identity,
            &root,
            62_000,
            62_001,
            Duration::from_secs(30),
        ))
        .expect_err("explicit legacy definer must block recovery");

    assert!(error.to_string().contains("explicit DEFINER identity"));
    assert!(!contains_manifest(&root));
    if root.exists() {
        std::fs::remove_dir_all(root).expect("remove rejected v7 MySQL fixture");
    }
}

#[test]
fn accepted_v7_image_selects_mysql_family_flavor_without_driver_guessing() {
    assert_eq!(
        super::accepted_v7_mysql_flavor(&accepted_inventory(), &source())
            .expect("accepted MySQL flavor"),
        MySqlFlavor::MySql
    );
    assert_eq!(
        super::accepted_v7_mysql_flavor(
            &accepted_inventory_with_image("registry.example/dev/mariadb:11.8"),
            &source(),
        )
        .expect("accepted MariaDB flavor"),
        MySqlFlavor::MariaDb
    );
}

#[test]
fn mysql_target_plan_binds_the_exact_durable_credential_secret() {
    let plan = MySqlLogicalResourcePlan::new(
        MySqlFlavor::MySql,
        "bill",
        "database",
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("MySQL target plan");
    let matching = target_credential();
    let mismatched = CredentialRecord::new(CredentialRecordOptions {
        credential_id: matching.credential_id().to_owned(),
        project_id: matching.project_id().map(str::to_owned),
        service_id: matching.service_id().to_owned(),
        username: matching.username().to_owned(),
        secret: "other-secret".to_owned(),
        lifecycle: matching.lifecycle(),
    });

    assert!(plan.matches_credential(&matching));
    assert!(!plan.matches_credential(&mismatched));
}

#[test]
fn engine_v7_mysql_retirement_forwards_the_accepted_resource_set() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("v7 MySQL retirement runtime");
    let source = source();
    let mut engine = RecordingContainerRetirement::default();
    let mut retirement = EngineV7MySqlSourceRetirement::new(&mut engine);

    runtime
        .block_on(retirement.retire_source(&source))
        .expect("retire accepted v7 MySQL resources");

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

impl V7MySqlSourceRetirement for RecordingRetirement {
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

struct RecordingV7MySqlExecutor {
    dump: Vec<u8>,
    v7_requests: Mutex<Vec<CommandRequest>>,
    restored_input: Arc<Mutex<Vec<u8>>>,
    input_complete: Arc<AtomicBool>,
    v8_starts: AtomicUsize,
}

impl RecordingV7MySqlExecutor {
    fn new(dump: Vec<u8>) -> Self {
        Self {
            dump,
            v7_requests: Mutex::new(Vec::new()),
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
        let database = request
            .arguments()
            .iter()
            .find_map(|argument| argument.strip_prefix("--database="));
        let output = match request.arguments().first().map(String::as_str) {
            Some("mysqldump" | "mariadb-dump") => self.dump.clone(),
            Some("mysql" | "mariadb") if database == Some("legacy_bill") => {
                b"legacy_bill\tlaravel\n".to_vec()
            }
            Some("mysql" | "mariadb") if database == Some("stackctl_bill_database") => {
                b"stackctl_bill_database\tst_bill_database\n".to_vec()
            }
            _ => Vec::new(),
        };
        let capture_restore = request
            .arguments()
            .iter()
            .any(|argument| argument == "--binary-mode");
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
                    .expect("drain v7 MySQL command input");
                if capture_restore {
                    *restored_input.lock().expect("restored input") = input;
                }
                input_complete.store(true, Ordering::Release);
            });
            let output: ContainerLogStream<'static> =
                Box::pin(stream::iter(vec![Ok(LogChunk::stdout(output))]));
            Ok(CommandSession::new(
                CommandExecutionId::new("v7-mysql-provider"),
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

impl CommandExecutor for RecordingV7MySqlExecutor {
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

impl V7ContainerCommandExecutor for RecordingV7MySqlExecutor {
    fn start_v7_command<'operation>(
        &'operation self,
        target: &'operation V7ContainerCommandTarget,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        self.v7_requests
            .lock()
            .expect("v7 MySQL requests")
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
        driver: "mysql".to_owned(),
        container_name: "bill-database".to_owned(),
        container_id: "legacy-mysql".to_owned(),
        named_volumes: vec!["bill-database-data".to_owned()],
        logical_data: BTreeMap::from([("database".to_owned(), "legacy_bill".to_owned())]),
    })
    .expect("v7 MySQL source")
}

fn accepted_inventory() -> AcceptedV7InventoryRecord {
    accepted_inventory_with_image("mysql:8.4")
}

fn accepted_inventory_with_image(image: &str) -> AcceptedV7InventoryRecord {
    let source_revision = format!("sha256:{}", "b".repeat(64));
    let inventory_json = serde_json::json!({
        "project_id": "bill",
        "canonical_project_path": "/work/bill",
        "source_revision": source_revision,
        "blockers": [],
        "services": [{
            "service_id": "database",
            "kind": "database",
            "driver": "mysql",
            "configured_image": image,
            "container_name": "bill-database",
            "observed_container_id": "legacy-mysql",
            "configured_mounts": [{
                "source_kind": "named_volume",
                "source": "bill-database-data",
                "target": "/var/lib/mysql",
                "read_only": false
            }],
            "observed_mounts": [{
                "source_kind": "named_volume",
                "source": "bill-database-data",
                "target": "/var/lib/mysql",
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
        accepted_at_unix_seconds: 60_999,
    })
    .expect("accepted v7 MySQL inventory")
}

fn target_logical_resource() -> LogicalResourceRecord {
    LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/database".to_owned(),
        shared_resource_id: "mysql-shared-8".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "mysql_database".to_owned(),
        compatibility_fingerprint: "sha256:mysql-8".to_owned(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
}

fn target_credential() -> CredentialRecord {
    CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/mysql".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "st_bill_database".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    })
}

fn administrator() -> CredentialRecord {
    CredentialRecord::new(CredentialRecordOptions {
        credential_id: "shared/mysql/bootstrap".to_owned(),
        project_id: None,
        service_id: "mysql".to_owned(),
        username: "root".to_owned(),
        secret: "root-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    })
}

fn owned_target_container() -> OwnedContainer {
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::SharedService,
        project_id: None,
        compatibility_fingerprint: "sha256:mysql-8".to_owned(),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("target metadata");
    let observed = ObservedContainer::new(ContainerId::new("mysql-target"), metadata.labels());

    reconstruct_owned_container(&observed, "install-1", 8).expect("owned MySQL container")
}

fn backup_root(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!("stackctl-{label}-{unique}"))
}

#[cfg(unix)]
fn contains_manifest(root: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };
    entries
        .filter_map(Result::ok)
        .flat_map(|entry| walk_files(entry.path()))
        .any(|path| path.file_name().is_some_and(|name| name == "manifest.json"))
}

#[cfg(unix)]
fn walk_files(path: PathBuf) -> Vec<PathBuf> {
    if path.is_file() {
        return vec![path];
    }
    std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .flat_map(|entry| walk_files(entry.path()))
        .collect()
}
