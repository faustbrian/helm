use super::{
    MySqlMigrationPreparationOptions, MySqlPreparationOptions, plan_mysql_project_resources,
    prepare_mysql_shared_instances, reconcile_mysql_migration_target,
    reconcile_prepared_mysql_instance,
};
use crate::control_plane::engine::{
    AttachedCommandOptions, BollardEngineAdapter, CommandRequest, ContainerDiscovery, EngineError,
    ImageResolver, ImmutableImageReference, InstallationResourceDeletionOptions,
    ManagedResourceMetadata, ManagedResourceMetadataOptions, NetworkCreateOptions, NetworkManager,
    OwnedContainer, ResourceKind, RetentionClass, delete_owned_installation_resources,
    reconstruct_owned_container, run_attached_command_capture,
};
use crate::control_plane::migration::{
    MigrationCutoverPlan, MigrationOperations, MigrationRollbackPlan, MySqlBackupOptions,
    MySqlDumpRestoreOptions, MySqlMigrationOperations, MySqlMigrationOperationsOptions,
    backup_mysql_database, restore_mysql_dump,
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

const MYSQL_IMAGE: &str = concat!(
    "mysql@sha256:",
    "c831a0f11348d402b43d77453e17d770be2eef356615a2823fe0f5a0d6c8b9af"
);
const MARIADB_IMAGE: &str = concat!(
    "mariadb@sha256:",
    "efb4959ef2c835cd735dbc388eb9ad6aab0c78dd64febcd51bc17481111890c4"
);

#[test]
#[ignore = "CI owns live shared MySQL isolation acceptance"]
fn live_docker_engine_two_projects_share_one_mysql_with_isolated_schemas() {
    run_live_engine_shared_database_isolation(LiveEngineDatabaseOptions {
        implementation: "mysql",
        display_name: "MySQL",
        major_version: "8",
        image: MYSQL_IMAGE,
        client_executable: "mysql",
    });
}

#[test]
#[ignore = "CI owns live shared MariaDB isolation acceptance"]
fn live_docker_engine_two_projects_share_one_mariadb_with_isolated_schemas() {
    run_live_engine_shared_database_isolation(LiveEngineDatabaseOptions {
        implementation: "mariadb",
        display_name: "MariaDB",
        major_version: "11",
        image: MARIADB_IMAGE,
        client_executable: "mariadb",
    });
}

fn run_live_engine_shared_database_isolation(options: LiveEngineDatabaseOptions<'_>) {
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
        architecture => panic!(
            "unsupported {} acceptance architecture '{architecture}'",
            options.display_name
        ),
    };
    let profile = CompatibilityProfile::from_options(CompatibilityFingerprintOptions {
        implementation: options.implementation.to_owned(),
        major_version: options.major_version.to_owned(),
        image_digest: options.image.to_owned(),
        extensions: Vec::new(),
        immutable_settings: BTreeMap::new(),
        persistence: PersistenceMode::Persistent,
        isolation: IsolationCapability::DatabaseAndRole,
        platform_architecture: Some(platform.to_owned()),
    })
    .unwrap_or_else(|error| panic!("build {} acceptance profile: {error}", options.display_name));
    let shared = plan_shared_instances(vec![
        SharedServiceRequest::new("bill", "database", profile.clone()),
        SharedServiceRequest::new("shop", "database", profile),
    ]);
    assert_eq!(shared.len(), 1, "compatible projects must share one plan");
    assert_eq!(shared[0].consumers().len(), 2);
    std::fs::create_dir(&state_directory).unwrap_or_else(|error| {
        panic!("create {} acceptance state: {error}", options.display_name)
    });
    let mut store =
        SqliteStateStore::open(&state_directory.join("state.sqlite3")).unwrap_or_else(|error| {
            panic!(
                "open {} acceptance state store: {error}",
                options.display_name
            )
        });
    let prepared = prepare_mysql_shared_instances(
        &mut store,
        &shared,
        &SequentialCredentialEntropy::new(0x31),
        MySqlPreparationOptions {
            installation_id: &installation_id,
            network_name: &network_name,
            schema_version: 8,
        },
    )
    .unwrap_or_else(|error| {
        panic!(
            "prepare shared {} acceptance resources: {error}",
            options.display_name
        )
    });
    let replayed = prepare_mysql_shared_instances(
        &mut store,
        &shared,
        &SequentialCredentialEntropy::new(0x71),
        MySqlPreparationOptions {
            installation_id: &installation_id,
            network_name: &network_name,
            schema_version: 8,
        },
    )
    .unwrap_or_else(|error| {
        panic!(
            "replay shared {} acceptance preparation: {error}",
            options.display_name
        )
    });
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
        .unwrap_or_else(|| panic!("prepared bill {} resources", options.display_name));
    let shop = prepared
        .projects()
        .iter()
        .find(|project| project.environment().project_id() == "shop")
        .unwrap_or_else(|| panic!("prepared shop {} resources", options.display_name));
    let network_metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.clone(),
        kind: ResourceKind::Network,
        project_id: None,
        compatibility_fingerprint: shared[0].fingerprint().as_str().to_owned(),
        schema_version: 8,
        desired_revision: shared[0].fingerprint().as_str().to_owned(),
        retention: RetentionClass::Persistent,
    })
    .unwrap_or_else(|error| {
        panic!(
            "build {} acceptance network metadata: {error}",
            options.display_name
        )
    });
    let network_request = NetworkCreateOptions::new(&network_name, network_metadata)
        .unwrap_or_else(|error| {
            panic!(
                "build {} acceptance network request: {error}",
                options.display_name
            )
        });
    let image = ImmutableImageReference::new(options.image).unwrap_or_else(|error| {
        panic!(
            "build immutable {} acceptance image reference: {error}",
            options.display_name
        )
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build MySQL acceptance runtime");
    let mut engine = runtime
        .block_on(BollardEngineAdapter::connect_unix(&socket))
        .expect("negotiate the selected Docker Engine API");

    runtime.block_on(async {
        engine.ensure_image(&image).await.unwrap_or_else(|error| {
            panic!(
                "resolve immutable {} acceptance image: {error}",
                options.display_name
            )
        });
        engine
            .create_network(&network_request)
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "create private {} acceptance network: {error}",
                    options.display_name
                )
            });
        let first = reconcile_prepared_mysql_instance(&mut engine, prepared, &installation_id, 8)
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "converge shared {} for two projects: {error}",
                    options.display_name
                )
            });
        assert!(
            first.logical_resource_drifts().is_empty(),
            "initial {} convergence reported drift: {:?}",
            options.display_name,
            first.logical_resource_drifts()
        );
        assert_eq!(first.logical_resources().len(), 2);
        let container = owned_shared_container(&engine, &installation_id).await;

        let bill_output = database_command(
            &engine,
            &container,
            options.client_executable,
            bill.credential().username(),
            bill.credential().secret(),
            bill.logical().schema_name(),
            "CREATE TABLE IF NOT EXISTS stackctl_acceptance(value varchar(64)); \
             TRUNCATE stackctl_acceptance; \
             INSERT INTO stackctl_acceptance VALUES ('bill-value'); \
             SELECT value FROM stackctl_acceptance;",
        )
        .await
        .unwrap_or_else(|error| {
            panic!(
                "write and read bill {} schema: {error}",
                options.display_name
            )
        });
        assert_eq!(bill_output, b"bill-value\n");
        let shop_output = database_command(
            &engine,
            &container,
            options.client_executable,
            shop.credential().username(),
            shop.credential().secret(),
            shop.logical().schema_name(),
            "CREATE TABLE IF NOT EXISTS stackctl_acceptance(value varchar(64)); \
             TRUNCATE stackctl_acceptance; \
             INSERT INTO stackctl_acceptance VALUES ('shop-value'); \
             SELECT value FROM stackctl_acceptance;",
        )
        .await
        .unwrap_or_else(|error| {
            panic!(
                "write and read shop {} schema: {error}",
                options.display_name
            )
        });
        assert_eq!(shop_output, b"shop-value\n");
        assert!(matches!(
            database_command(
                &engine,
                &container,
                options.client_executable,
                shop.credential().username(),
                shop.credential().secret(),
                bill.logical().schema_name(),
                "SELECT 1;",
            )
            .await,
            Err(EngineError::ContainerExit { .. })
        ));

        let dump_path = state_directory.join("explicit-sandbox.sql");
        std::fs::write(
            &dump_path,
            b"CREATE TABLE stackctl_acceptance(value varchar(64));\n\
              INSERT INTO stackctl_acceptance VALUES ('dump-restored');\n",
        )
        .unwrap_or_else(|error| {
            panic!(
                "write explicit {} dump acceptance input: {error}",
                options.display_name
            )
        });
        restore_mysql_dump(
            &engine,
            &container,
            &MySqlDumpRestoreOptions {
                flavor: prepared.instance().flavor(),
                logical: bill.logical(),
                credential: bill.credential(),
                administrator: prepared.instance().bootstrap_credential(),
                file: &dump_path,
                reset: true,
                timeout: Duration::from_secs(30),
            },
        )
        .await
        .unwrap_or_else(|error| {
            panic!(
                "restore explicit bill {} dump: {error}",
                options.display_name
            )
        });
        assert_eq!(
            database_command(
                &engine,
                &container,
                options.client_executable,
                bill.credential().username(),
                bill.credential().secret(),
                bill.logical().schema_name(),
                "SELECT value FROM stackctl_acceptance;",
            )
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "read explicit bill {} dump restore: {error}",
                    options.display_name
                )
            }),
            b"dump-restored\n"
        );
        assert_eq!(
            database_command(
                &engine,
                &container,
                options.client_executable,
                shop.credential().username(),
                shop.credential().secret(),
                shop.logical().schema_name(),
                "SELECT value FROM stackctl_acceptance;",
            )
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "read sibling shop {} after explicit dump restore: {error}",
                    options.display_name
                )
            }),
            b"shop-value\n"
        );

        let source_logical = first
            .logical_resources()
            .iter()
            .find(|logical| logical.project_id() == "bill")
            .unwrap_or_else(|| {
                panic!("find active bill {} logical resource", options.display_name)
            });
        let backup_root = state_directory.join("backups");
        let backup = backup_mysql_database(
            &engine,
            &container,
            &MySqlBackupOptions {
                flavor: prepared.instance().flavor(),
                logical_resource: source_logical,
                credential: bill.credential(),
                database_name: bill.logical().schema_name(),
                installation_id: &installation_id,
                created_at_unix_seconds: 11_500,
                backup_root: &backup_root,
                timeout: Duration::from_secs(30),
            },
        )
        .await
        .unwrap_or_else(|error| {
            panic!(
                "back up live bill {} database: {error}",
                options.display_name
            )
        });
        database_command(
            &engine,
            &container,
            options.client_executable,
            bill.credential().username(),
            bill.credential().secret(),
            bill.logical().schema_name(),
            "UPDATE stackctl_acceptance SET value = 'mutated-after-backup';",
        )
        .await
        .unwrap_or_else(|error| {
            panic!(
                "mutate bill {} source after backup: {error}",
                options.display_name
            )
        });
        let backup_checkpoint = MigrationRecord::new(MigrationRecordOptions {
            migration_id: format!("bill-database-{}-recovery", options.implementation),
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
        .unwrap_or_else(|error| {
            panic!(
                "record live {} backup checkpoint: {error}",
                options.display_name
            )
        });
        let target = reconcile_mysql_migration_target(
            &mut store,
            &mut engine,
            &shared[0],
            &SequentialCredentialEntropy::new(0x81),
            MySqlMigrationPreparationOptions {
                migration_id: backup_checkpoint.migration_id(),
                project_id: "bill",
                installation_id: &installation_id,
                network_name: &network_name,
                schema_version: 8,
                desired_revision: source_logical.desired_revision(),
            },
        )
        .await
        .unwrap_or_else(|error| {
            panic!(
                "reconcile isolated {} recovery target: {error}",
                options.display_name
            )
        });
        let target_resources = plan_mysql_project_resources(
            "bill",
            "database",
            target.plan(),
            CredentialSecret::new(bill.credential().secret().to_owned()),
        )
        .unwrap_or_else(|error| {
            panic!(
                "plan {} recovery target resources: {error}",
                options.display_name
            )
        });
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
                .unwrap_or_else(|error| {
                    panic!("plan {} recovery cutover: {error}", options.display_name)
                });
        let rollback =
            MigrationRollbackPlan::new(project, bill.environment().clone(), vec![retained_target])
                .unwrap_or_else(|error| {
                    panic!("plan {} recovery rollback: {error}", options.display_name)
                });
        let mut operations = MySqlMigrationOperations::new(
            &engine,
            MySqlMigrationOperationsOptions {
                flavor: prepared.instance().flavor(),
                source_container: &container,
                target_container: target.container(),
                source_logical_resource: source_logical,
                target_logical_resource: &target_logical,
                source_credential: bill.credential(),
                target_credential: target_resources.credential(),
                source_administrator: prepared.instance().bootstrap_credential(),
                source_environment: bill.environment(),
                target_instance: target.plan(),
                target_plan: target_resources.logical(),
                installation_id: &installation_id,
                backup_root: &backup_root,
                operation_unix_seconds: 11_700,
                timeout: Duration::from_secs(30),
                cutover,
                rollback,
            },
        )
        .unwrap_or_else(|error| {
            panic!(
                "prepare live {} migration operations: {error}",
                options.display_name
            )
        });
        let target_plan = operations
            .provision_target(&backup_checkpoint)
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "provision isolated {} recovery database: {error}",
                    options.display_name
                )
            });
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
        .unwrap_or_else(|error| {
            panic!(
                "record live {} target checkpoint: {error}",
                options.display_name
            )
        });
        operations
            .restore(
                &restore_checkpoint,
                backup.reference(),
                target_plan.target_resource_id(),
            )
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "restore verified {} backup into isolated target: {error}",
                    options.display_name
                )
            });
        assert_eq!(
            database_command(
                &engine,
                target.container(),
                options.client_executable,
                target_resources.credential().username(),
                target_resources.credential().secret(),
                target_plan.target_resource_id(),
                "SELECT value FROM stackctl_acceptance;",
            )
            .await
            .unwrap_or_else(|error| {
                panic!("read restored {} target: {error}", options.display_name)
            }),
            b"dump-restored\n",
            "target restore must reproduce the verified point-in-time backup"
        );
        assert_eq!(
            database_command(
                &engine,
                &container,
                options.client_executable,
                shop.credential().username(),
                shop.credential().secret(),
                shop.logical().schema_name(),
                "SELECT value FROM stackctl_acceptance;",
            )
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "read sibling {} database after restore: {error}",
                    options.display_name
                )
            }),
            b"shop-value\n"
        );
        drop(operations);

        let second = reconcile_prepared_mysql_instance(&mut engine, prepared, &installation_id, 8)
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "reconcile unchanged shared {} instance: {error}",
                    options.display_name
                )
            });
        assert!(second.logical_resource_drifts().is_empty());
        let replayed_container = owned_shared_container(&engine, &installation_id).await;
        assert_eq!(replayed_container.id(), container.id());

        let bill_logical = first
            .logical_resources()
            .iter()
            .find(|logical| logical.project_id() == "bill")
            .map(orphaned_logical_resource)
            .unwrap_or_else(|| panic!("find bill logical {} resource", options.display_name));
        let credentials = [
            disabled_credential(bill.credential()),
            prepared.instance().bootstrap_credential().clone(),
        ];
        let observed = engine.discover_managed().await.unwrap_or_else(|error| {
            panic!(
                "discover {} lifecycle acceptance resources: {error}",
                options.display_name
            )
        });
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
        .unwrap_or_else(|error| {
            panic!(
                "revoke removed bill {} access: {error}",
                options.display_name
            )
        });
        assert_eq!(revoked, 1);
        assert!(matches!(
            database_command(
                &engine,
                &container,
                options.client_executable,
                bill.credential().username(),
                bill.credential().secret(),
                bill.logical().schema_name(),
                "SELECT value FROM stackctl_acceptance;",
            )
            .await,
            Err(EngineError::ContainerExit { .. })
        ));
        assert_eq!(
            database_command(
                &engine,
                &container,
                options.client_executable,
                shop.credential().username(),
                shop.credential().secret(),
                shop.logical().schema_name(),
                "SELECT value FROM stackctl_acceptance;",
            )
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "read shop {} data after bill removal: {error}",
                    options.display_name
                )
            }),
            b"shop-value\n"
        );
        assert_eq!(
            database_command(
                &engine,
                &container,
                options.client_executable,
                prepared.instance().bootstrap_credential().username(),
                prepared.instance().bootstrap_credential().secret(),
                bill.logical().schema_name(),
                "SELECT value FROM stackctl_acceptance;",
            )
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "verify retained bill {} data as root: {error}",
                    options.display_name
                )
            }),
            b"mutated-after-backup\n",
            "isolated recovery must not mutate the retained source database"
        );

        let restored =
            reconcile_prepared_mysql_instance(&mut engine, prepared, &installation_id, 8)
                .await
                .unwrap_or_else(|error| {
                    panic!(
                        "restore bill {} access from active configuration: {error}",
                        options.display_name
                    )
                });
        assert!(restored.logical_resource_drifts().is_empty());
        assert_eq!(
            database_command(
                &engine,
                &container,
                options.client_executable,
                bill.credential().username(),
                bill.credential().secret(),
                bill.logical().schema_name(),
                "SELECT value FROM stackctl_acceptance;",
            )
            .await
            .unwrap_or_else(|error| {
                panic!("read restored bill {} data: {error}", options.display_name)
            }),
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
                        .unwrap_or_else(|| panic!("persistent {} volume", options.display_name))
                        .name()
                        .to_owned(),
                    target.volume().name().to_owned(),
                ],
            },
        )
        .await
        .unwrap_or_else(|error| {
            panic!(
                "delete shared {} acceptance resources: {error}",
                options.display_name
            )
        });
    });

    drop(store);
    std::fs::remove_dir_all(&state_directory).unwrap_or_else(|error| {
        panic!("remove {} acceptance state: {error}", options.display_name)
    });
    println!(
        "shared {} isolation acceptance passed for {installation_id}",
        options.display_name
    );
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
        .expect("discover shared database acceptance resources")
        .into_iter()
        .filter_map(|observed| reconstruct_owned_container(&observed, installation_id, 8).ok())
        .find(|owned| owned.metadata().kind() == ResourceKind::SharedService)
        .expect("discover one owned shared database container")
}

async fn database_command(
    engine: &BollardEngineAdapter,
    container: &OwnedContainer,
    client_executable: &str,
    username: &str,
    password: &str,
    schema: &str,
    sql: &str,
) -> Result<Vec<u8>, EngineError> {
    let request = CommandRequest::new(
        vec![
            client_executable.to_owned(),
            "--protocol=socket".to_owned(),
            format!("--user={username}"),
            "--batch".to_owned(),
            "--skip-column-names".to_owned(),
            format!("--database={schema}"),
            format!("--execute={sql}"),
        ],
        BTreeMap::from([("MYSQL_PWD".to_owned(), password.to_owned())]),
        None,
    )?;
    let options = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "exercise MySQL-family tenant isolation",
        Duration::from_secs(15),
    )?;

    run_attached_command_capture(engine, container, &options).await
}

#[derive(Clone, Copy)]
struct LiveEngineDatabaseOptions<'a> {
    implementation: &'a str,
    display_name: &'a str,
    major_version: &'a str,
    image: &'a str,
    client_executable: &'a str,
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
