use super::{
    MongoDbMigrationPreparationOptions, MongoDbPreparationOptions, plan_mongodb_project_resources,
    prepare_mongodb_shared_instances, reconcile_mongodb_migration_target,
    reconcile_prepared_mongodb_instance,
};
use crate::control_plane::engine::{
    AttachedCommandOptions, BollardEngineAdapter, CommandRequest, ContainerDiscovery, EngineError,
    ImageResolver, ImmutableImageReference, InstallationResourceDeletionOptions,
    ManagedResourceMetadata, ManagedResourceMetadataOptions, NetworkCreateOptions, NetworkManager,
    OwnedContainer, ResourceKind, RetentionClass, delete_owned_installation_resources,
    reconstruct_owned_container, run_attached_command_capture,
};
use crate::control_plane::migration::{
    MigrationCutoverPlan, MigrationOperations, MigrationRollbackPlan, MongoDbBackupOptions,
    MongoDbMigrationOperations, MongoDbMigrationOperationsOptions, backup_mongodb_database,
};
use crate::control_plane::shared_infrastructure::{
    CompatibilityFingerprintOptions, CompatibilityProfile, CredentialEntropy,
    CredentialGenerationError, CredentialSecret, IsolationCapability, OrphanedSharedAccessOptions,
    PersistenceMode, SharedServiceRequest, plan_shared_instances,
    revoke_orphaned_shared_access_from_observed,
};
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
    LogicalResourceRecordOptions, MigrationPhase, MigrationRecord, MigrationRecordOptions,
    ProjectRecord, ResourceLifecycle, SqliteStateStore,
};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MONGODB_IMAGE: &str = concat!(
    "mongo@sha256:",
    "ffa440e8d62533e24a67696ae1bbb46e610ebb3167d65abd122b496ae06d28e6"
);

#[test]
#[ignore = "CI owns live shared MongoDB isolation acceptance"]
fn live_docker_engine_two_projects_share_one_mongodb_with_isolated_databases() {
    let socket = std::env::var_os("STACKCTL_ENGINE_SOCKET")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/var/run/docker.sock"));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must follow the Unix epoch")
        .as_nanos();
    let installation_id = format!("ci-{}-{nonce}", std::process::id());
    let network_name = format!("stackctl-{installation_id}");
    let state_directory = std::env::temp_dir().join(format!("stackctl-{installation_id}"));
    let platform = match std::env::consts::ARCH {
        "aarch64" => "linux/arm64",
        "x86_64" => "linux/amd64",
        architecture => panic!("unsupported MongoDB acceptance architecture '{architecture}'"),
    };
    let profile = CompatibilityProfile::from_options(CompatibilityFingerprintOptions {
        implementation: "mongodb".to_owned(),
        major_version: "8".to_owned(),
        image_digest: MONGODB_IMAGE.to_owned(),
        extensions: Vec::new(),
        immutable_settings: BTreeMap::new(),
        persistence: PersistenceMode::Persistent,
        isolation: IsolationCapability::DatabaseAndRole,
        platform_architecture: Some(platform.to_owned()),
    })
    .expect("build MongoDB acceptance profile");
    let shared = plan_shared_instances(vec![
        SharedServiceRequest::new("bill", "database", profile.clone()),
        SharedServiceRequest::new("shop", "database", profile),
    ]);
    assert_eq!(shared.len(), 1, "compatible projects must share one plan");
    assert_eq!(shared[0].consumers().len(), 2);
    std::fs::create_dir(&state_directory).expect("create MongoDB acceptance state");
    let mut store = SqliteStateStore::open(&state_directory.join("state.sqlite3"))
        .expect("open MongoDB acceptance state store");
    let prepared = prepare_mongodb_shared_instances(
        &mut store,
        &shared,
        &SequentialCredentialEntropy::new(0x41),
        MongoDbPreparationOptions {
            installation_id: &installation_id,
            network_name: &network_name,
            schema_version: 8,
        },
    )
    .expect("prepare shared MongoDB acceptance resources");
    let replayed = prepare_mongodb_shared_instances(
        &mut store,
        &shared,
        &SequentialCredentialEntropy::new(0x81),
        MongoDbPreparationOptions {
            installation_id: &installation_id,
            network_name: &network_name,
            schema_version: 8,
        },
    )
    .expect("replay shared MongoDB acceptance preparation");
    let prepared_credentials = prepared[0]
        .projects()
        .iter()
        .map(|project| project.credential().clone())
        .collect::<Vec<_>>();
    let replayed_credentials = replayed[0]
        .projects()
        .iter()
        .map(|project| project.credential().clone())
        .collect::<Vec<_>>();
    assert_eq!(prepared_credentials, replayed_credentials);
    assert_ne!(
        prepared_credentials[0].secret(),
        prepared_credentials[1].secret()
    );
    let prepared = &prepared[0];
    let bill = prepared
        .projects()
        .iter()
        .find(|project| project.environment().project_id() == "bill")
        .expect("prepared bill MongoDB resources");
    let shop = prepared
        .projects()
        .iter()
        .find(|project| project.environment().project_id() == "shop")
        .expect("prepared shop MongoDB resources");
    let network_metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.clone(),
        kind: ResourceKind::Network,
        project_id: None,
        compatibility_fingerprint: shared[0].fingerprint().as_str().to_owned(),
        schema_version: 8,
        desired_revision: shared[0].fingerprint().as_str().to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("build MongoDB acceptance network metadata");
    let network_request = NetworkCreateOptions::new(&network_name, network_metadata)
        .expect("build MongoDB acceptance network request");
    let image = ImmutableImageReference::new(MONGODB_IMAGE)
        .expect("build immutable MongoDB acceptance image reference");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build MongoDB acceptance runtime");
    let mut engine = runtime
        .block_on(BollardEngineAdapter::connect_unix(&socket))
        .expect("negotiate the selected Docker Engine API");

    runtime.block_on(async {
        engine
            .ensure_image(&image)
            .await
            .expect("resolve immutable MongoDB acceptance image");
        engine
            .create_network(&network_request)
            .await
            .expect("create private MongoDB acceptance network");
        let first = reconcile_prepared_mongodb_instance(&mut engine, prepared, &installation_id, 8)
            .await
            .expect("converge shared MongoDB for two projects");
        assert!(
            first.logical_resource_drifts().is_empty(),
            "initial MongoDB convergence reported drift: {:?}",
            first.logical_resource_drifts()
        );
        assert_eq!(first.logical_resources().len(), 2);
        let container = owned_shared_container(&engine, &installation_id).await;

        let bill_output = mongodb_write_and_read(
            &engine,
            &container,
            bill.logical().database_name(),
            bill.credential().username(),
            bill.credential().secret(),
            "bill-value",
        )
        .await
        .expect("write and read bill MongoDB database");
        assert_eq!(bill_output, b"bill-value\n");
        let shop_output = mongodb_write_and_read(
            &engine,
            &container,
            shop.logical().database_name(),
            shop.credential().username(),
            shop.credential().secret(),
            "shop-value",
        )
        .await
        .expect("write and read shop MongoDB database");
        assert_eq!(shop_output, b"shop-value\n");
        assert!(matches!(
            mongodb_cross_database_read(
                &engine,
                &container,
                shop.logical().database_name(),
                shop.credential().username(),
                shop.credential().secret(),
                bill.logical().database_name(),
            )
            .await,
            Err(EngineError::ContainerExit { .. })
        ));

        let source_logical = first
            .logical_resources()
            .iter()
            .find(|logical| logical.project_id() == "bill")
            .expect("find active bill MongoDB logical resource");
        let backup_root = state_directory.join("backups");
        let backup = backup_mongodb_database(
            &engine,
            &container,
            &MongoDbBackupOptions {
                logical_resource: source_logical,
                credential: bill.credential(),
                database_name: bill.logical().database_name(),
                installation_id: &installation_id,
                created_at_unix_seconds: 11_500,
                backup_root: &backup_root,
                timeout: Duration::from_secs(30),
            },
        )
        .await
        .expect("back up live bill MongoDB database");
        assert_eq!(
            mongodb_write_and_read(
                &engine,
                &container,
                bill.logical().database_name(),
                bill.credential().username(),
                bill.credential().secret(),
                "mutated-after-backup",
            )
            .await
            .expect("mutate bill MongoDB source after backup"),
            b"mutated-after-backup\n"
        );
        let backup_checkpoint = MigrationRecord::new(MigrationRecordOptions {
            migration_id: "bill-database-mongodb-recovery".to_owned(),
            project_id: "bill".to_owned(),
            source_revision: source_logical.desired_revision().to_owned(),
            target_revision: source_logical.desired_revision().to_owned(),
            source_compatibility_fingerprint: source_logical.compatibility_fingerprint().to_owned(),
            target_compatibility_fingerprint: source_logical.compatibility_fingerprint().to_owned(),
            phase: MigrationPhase::BackupVerified,
            backup_reference: Some(backup.reference().to_owned()),
            backup_artifact_sha256: Some(backup.artifact_sha256().to_owned()),
            backup_artifact_size_bytes: Some(backup.artifact_size_bytes()),
            target_resource_id: None,
            rollback_reference: Some(source_logical.shared_resource_id().to_owned()),
            updated_at_unix_seconds: 11_500,
        })
        .expect("record live MongoDB backup checkpoint");
        let target = reconcile_mongodb_migration_target(
            &mut store,
            &mut engine,
            &shared[0],
            &SequentialCredentialEntropy::new(0x91),
            MongoDbMigrationPreparationOptions {
                migration_id: backup_checkpoint.migration_id(),
                project_id: "bill",
                installation_id: &installation_id,
                network_name: &network_name,
                schema_version: 8,
                desired_revision: source_logical.desired_revision(),
            },
        )
        .await
        .expect("reconcile isolated MongoDB recovery target");
        let target_resources = plan_mongodb_project_resources(
            "bill",
            "database",
            target.plan(),
            CredentialSecret::new(bill.credential().secret().to_owned()),
        )
        .expect("plan MongoDB recovery target resources");
        let target_logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
            logical_resource_id: format!(
                "{}/{}",
                source_logical.project_id(),
                source_logical.service_id()
            ),
            shared_resource_id: target.volume().name().to_owned(),
            project_id: source_logical.project_id().to_owned(),
            service_id: source_logical.service_id().to_owned(),
            kind: source_logical.kind().to_owned(),
            compatibility_fingerprint: source_logical.compatibility_fingerprint().to_owned(),
            desired_revision: target_resources.environment().revision().to_owned(),
            lifecycle: ResourceLifecycle::Active,
            orphaned_at_unix_seconds: None,
        });
        let project = ProjectRecord::new(
            state_directory.join("projects/bill"),
            "bill".to_owned(),
            vec!["bill-app.stackctl.localhost".to_owned()],
        );
        let retained_target = LogicalResourceRecord::new(LogicalResourceRecordOptions {
            logical_resource_id: target_logical.logical_resource_id().to_owned(),
            shared_resource_id: target_logical.shared_resource_id().to_owned(),
            project_id: target_logical.project_id().to_owned(),
            service_id: target_logical.service_id().to_owned(),
            kind: target_logical.kind().to_owned(),
            compatibility_fingerprint: target_logical.compatibility_fingerprint().to_owned(),
            desired_revision: target_logical.desired_revision().to_owned(),
            lifecycle: ResourceLifecycle::Retained,
            orphaned_at_unix_seconds: None,
        });
        let cutover =
            MigrationCutoverPlan::new(project.clone(), target_resources.environment().clone())
                .expect("plan MongoDB recovery cutover");
        let rollback =
            MigrationRollbackPlan::new(project, bill.environment().clone(), vec![retained_target])
                .expect("plan MongoDB recovery rollback");
        let mut operations = MongoDbMigrationOperations::new(
            &engine,
            MongoDbMigrationOperationsOptions {
                source_container: &container,
                target_container: target.container(),
                source_logical_resource: source_logical,
                target_logical_resource: &target_logical,
                source_credential: bill.credential(),
                target_credential: target_resources.credential(),
                source_administrator: prepared.instance().bootstrap_credential(),
                target_administrator: target.bootstrap_credential(),
                source_environment: bill.environment(),
                target_plan: target_resources.logical(),
                installation_id: &installation_id,
                backup_root: &backup_root,
                operation_unix_seconds: 11_700,
                timeout: Duration::from_secs(30),
                cutover,
                rollback,
            },
        )
        .expect("prepare live MongoDB migration operations");
        let target_plan = operations
            .provision_target(&backup_checkpoint)
            .await
            .expect("provision isolated MongoDB recovery database");
        let restore_checkpoint = MigrationRecord::new(MigrationRecordOptions {
            migration_id: backup_checkpoint.migration_id().to_owned(),
            project_id: backup_checkpoint.project_id().to_owned(),
            source_revision: backup_checkpoint.source_revision().to_owned(),
            target_revision: target_logical.desired_revision().to_owned(),
            source_compatibility_fingerprint: backup_checkpoint
                .source_compatibility_fingerprint()
                .to_owned(),
            target_compatibility_fingerprint: backup_checkpoint
                .target_compatibility_fingerprint()
                .to_owned(),
            phase: MigrationPhase::TargetProvisioned,
            backup_reference: backup_checkpoint.backup_reference().map(str::to_owned),
            backup_artifact_sha256: backup_checkpoint
                .backup_artifact_sha256()
                .map(str::to_owned),
            backup_artifact_size_bytes: backup_checkpoint.backup_artifact_size_bytes(),
            target_resource_id: Some(target_plan.target_resource_id().to_owned()),
            rollback_reference: backup_checkpoint.rollback_reference().map(str::to_owned),
            updated_at_unix_seconds: 11_600,
        })
        .expect("record live MongoDB target checkpoint");
        operations
            .restore(
                &restore_checkpoint,
                backup.reference(),
                target_plan.target_resource_id(),
            )
            .await
            .expect("restore verified MongoDB backup into isolated target");
        assert_eq!(
            mongodb_read(
                &engine,
                target.container(),
                target_plan.target_resource_id(),
                target_resources.credential().username(),
                target_resources.credential().secret(),
                target_plan.target_resource_id(),
            )
            .await
            .expect("read restored MongoDB target"),
            b"bill-value\n",
            "target restore must reproduce the verified point-in-time backup"
        );
        assert_eq!(
            mongodb_read(
                &engine,
                &container,
                shop.logical().database_name(),
                shop.credential().username(),
                shop.credential().secret(),
                shop.logical().database_name(),
            )
            .await
            .expect("read sibling MongoDB database after restore"),
            b"shop-value\n"
        );
        drop(operations);

        let second =
            reconcile_prepared_mongodb_instance(&mut engine, prepared, &installation_id, 8)
                .await
                .expect("reconcile unchanged shared MongoDB instance");
        assert!(second.logical_resource_drifts().is_empty());
        let replayed_container = owned_shared_container(&engine, &installation_id).await;
        assert_eq!(replayed_container.id(), container.id());

        let bill_logical = first
            .logical_resources()
            .iter()
            .find(|logical| logical.project_id() == "bill")
            .map(orphaned_logical_resource)
            .expect("find bill logical MongoDB resource");
        let credentials = [
            disabled_credential(bill.credential()),
            prepared.instance().bootstrap_credential().clone(),
        ];
        let observed = engine
            .discover_managed()
            .await
            .expect("discover MongoDB lifecycle acceptance resources");
        let revoked = revoke_orphaned_shared_access_from_observed(
            &mut engine,
            &observed,
            OrphanedSharedAccessOptions {
                resources: first.physical_resources(),
                logical_resources: std::slice::from_ref(&bill_logical),
                credentials: &credentials,
                installation_id: &installation_id,
                schema_version: 8,
                timeout: Duration::from_secs(15),
            },
        )
        .await
        .expect("revoke removed bill MongoDB access");
        assert_eq!(revoked, 1);
        assert!(matches!(
            mongodb_read(
                &engine,
                &container,
                bill.logical().database_name(),
                bill.credential().username(),
                bill.credential().secret(),
                bill.logical().database_name(),
            )
            .await,
            Err(EngineError::ContainerExit { .. })
        ));
        assert_eq!(
            mongodb_read(
                &engine,
                &container,
                shop.logical().database_name(),
                shop.credential().username(),
                shop.credential().secret(),
                shop.logical().database_name(),
            )
            .await
            .expect("read shop MongoDB data after bill removal"),
            b"shop-value\n"
        );
        assert_eq!(
            mongodb_read(
                &engine,
                &container,
                "admin",
                prepared.instance().bootstrap_credential().username(),
                prepared.instance().bootstrap_credential().secret(),
                bill.logical().database_name(),
            )
            .await
            .expect("verify retained bill MongoDB data as administrator"),
            b"mutated-after-backup\n",
            "isolated recovery must not mutate the retained source database"
        );

        let restored =
            reconcile_prepared_mongodb_instance(&mut engine, prepared, &installation_id, 8)
                .await
                .expect("restore bill MongoDB access from active configuration");
        assert!(restored.logical_resource_drifts().is_empty());
        assert_eq!(
            mongodb_read(
                &engine,
                &container,
                bill.logical().database_name(),
                bill.credential().username(),
                bill.credential().secret(),
                bill.logical().database_name(),
            )
            .await
            .expect("read restored bill MongoDB data"),
            b"mutated-after-backup\n"
        );
        let restored_container = owned_shared_container(&engine, &installation_id).await;
        assert_eq!(restored_container.id(), container.id());

        delete_owned_installation_resources(
            &mut engine,
            InstallationResourceDeletionOptions {
                installation_id: &installation_id,
                schema_version: 8,
                authorized_persistent_volumes: &[
                    prepared
                        .instance()
                        .volume()
                        .expect("persistent MongoDB volume")
                        .name()
                        .to_owned(),
                    target.volume().name().to_owned(),
                ],
            },
        )
        .await
        .expect("delete shared MongoDB acceptance resources");
    });

    drop(store);
    std::fs::remove_dir_all(&state_directory).expect("remove MongoDB acceptance state");
    println!("shared MongoDB isolation acceptance passed for {installation_id}");
}

fn orphaned_logical_resource(logical: &LogicalResourceRecord) -> LogicalResourceRecord {
    LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: logical.logical_resource_id().to_owned(),
        shared_resource_id: logical.shared_resource_id().to_owned(),
        project_id: logical.project_id().to_owned(),
        service_id: logical.service_id().to_owned(),
        kind: logical.kind().to_owned(),
        compatibility_fingerprint: logical.compatibility_fingerprint().to_owned(),
        desired_revision: logical.desired_revision().to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(12_345),
    })
}

fn disabled_credential(credential: &CredentialRecord) -> CredentialRecord {
    CredentialRecord::new(CredentialRecordOptions {
        credential_id: credential.credential_id().to_owned(),
        project_id: credential.project_id().map(str::to_owned),
        service_id: credential.service_id().to_owned(),
        username: credential.username().to_owned(),
        secret: credential.secret().to_owned(),
        lifecycle: CredentialLifecycle::Disabled,
    })
}

async fn owned_shared_container(
    engine: &BollardEngineAdapter,
    installation_id: &str,
) -> OwnedContainer {
    engine
        .discover_managed()
        .await
        .expect("discover shared MongoDB acceptance resources")
        .into_iter()
        .filter_map(|observed| reconstruct_owned_container(&observed, installation_id, 8).ok())
        .find(|owned| owned.metadata().kind() == ResourceKind::SharedService)
        .expect("discover one owned shared MongoDB container")
}

async fn mongodb_write_and_read(
    engine: &BollardEngineAdapter,
    container: &OwnedContainer,
    database: &str,
    username: &str,
    password: &str,
    value: &str,
) -> Result<Vec<u8>, EngineError> {
    let database = json(database);
    let username = json(username);
    let password = json(password);
    let value = json(value);
    let script = format!(
        "try {{\n\
         const target = connect(\"mongodb://127.0.0.1:27017/\" + {database});\n\
         if (!target.auth({username}, {password})) {{ quit(1); }}\n\
         target.stackctl_acceptance.deleteMany({{}});\n\
         target.stackctl_acceptance.insertOne({{ value: {value} }});\n\
         print(target.stackctl_acceptance.findOne({{}}).value);\n\
         }} catch (error) {{ print(error); quit(1); }}\n"
    );

    mongodb_script(engine, container, script).await
}

async fn mongodb_cross_database_read(
    engine: &BollardEngineAdapter,
    container: &OwnedContainer,
    authentication_database: &str,
    username: &str,
    password: &str,
    target_database: &str,
) -> Result<Vec<u8>, EngineError> {
    let authentication_database = json(authentication_database);
    let username = json(username);
    let password = json(password);
    let target_database = json(target_database);
    let script = format!(
        "try {{\n\
         const auth = connect(\"mongodb://127.0.0.1:27017/\" + {authentication_database});\n\
         if (!auth.auth({username}, {password})) {{ quit(1); }}\n\
         auth.getSiblingDB({target_database}).stackctl_acceptance.findOne({{}});\n\
         quit(0);\n\
         }} catch (error) {{ quit(1); }}\n"
    );

    mongodb_script(engine, container, script).await
}

async fn mongodb_read(
    engine: &BollardEngineAdapter,
    container: &OwnedContainer,
    authentication_database: &str,
    username: &str,
    password: &str,
    target_database: &str,
) -> Result<Vec<u8>, EngineError> {
    let authentication_database = json(authentication_database);
    let username = json(username);
    let password = json(password);
    let target_database = json(target_database);
    let script = format!(
        "try {{\n\
         const auth = connect(\"mongodb://127.0.0.1:27017/\" + {authentication_database});\n\
         if (!auth.auth({username}, {password})) {{ quit(1); }}\n\
         print(auth.getSiblingDB({target_database}).stackctl_acceptance.findOne({{}}).value);\n\
         }} catch (error) {{ print(error); quit(1); }}\n"
    );

    mongodb_script(engine, container, script).await
}

async fn mongodb_script(
    engine: &BollardEngineAdapter,
    container: &OwnedContainer,
    script: String,
) -> Result<Vec<u8>, EngineError> {
    let request = CommandRequest::new(
        vec![
            "mongosh".to_owned(),
            "--quiet".to_owned(),
            "--nodb".to_owned(),
            "--file".to_owned(),
            "/dev/stdin".to_owned(),
        ],
        BTreeMap::new(),
        None,
    )?;
    let options = AttachedCommandOptions::new(
        request,
        script.into_bytes(),
        "exercise MongoDB tenant isolation",
        Duration::from_secs(15),
    )?;

    run_attached_command_capture(engine, container, &options).await
}

fn json(value: &str) -> String {
    serde_json::to_string(value).expect("encode MongoDB acceptance value")
}

struct SequentialCredentialEntropy {
    next: AtomicU8,
}

impl SequentialCredentialEntropy {
    const fn new(first: u8) -> Self {
        Self {
            next: AtomicU8::new(first),
        }
    }
}

impl CredentialEntropy for SequentialCredentialEntropy {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), CredentialGenerationError> {
        bytes.fill(self.next.fetch_add(1, Ordering::SeqCst));

        Ok(())
    }
}
