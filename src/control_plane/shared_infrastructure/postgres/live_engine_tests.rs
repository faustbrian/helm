use super::{
    PostgresMigrationPreparationOptions, PostgresPreparationOptions,
    plan_postgres_project_resources, prepare_postgres_shared_instances,
    reconcile_postgres_migration_target, reconcile_prepared_postgres_instance,
};
use crate::control_plane::application::{ProjectSource, plan_project_registry};
use crate::control_plane::engine::{
    AttachedCommandOptions, BollardEngineAdapter, CommandRequest, ContainerDiscovery, EngineError,
    ImageResolver, ImmutableImageReference, InstallationResourceDeletionOptions,
    ManagedResourceMetadata, ManagedResourceMetadataOptions, NetworkCreateOptions, NetworkManager,
    OwnedContainer, ResourceKind, RetentionClass, delete_owned_installation_resources,
    reconstruct_owned_container, run_attached_command_capture,
};
use crate::control_plane::migration::{
    EnginePostgresSourceRetirement, MigrationCutoverPlan, MigrationOperations,
    MigrationRollbackPlan, PostgresBackupOptions, PostgresMigrationOperations,
    PostgresMigrationOperationsOptions, PostgresSourceRetirementOptions, backup_postgres_database,
};
use crate::control_plane::shared_infrastructure::{
    CompatibilityFingerprintOptions, CompatibilityProfile, CredentialEntropy,
    CredentialGenerationError, CredentialSecret, IsolationCapability, OrphanedSharedAccessOptions,
    PersistenceMode, SharedServiceRequest, plan_shared_instances,
    resolve_execution_shared_instances, revoke_orphaned_shared_access_from_observed,
};
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
    LogicalResourceRecordOptions, MigrationPhase, MigrationRecord, MigrationRecordOptions,
    ProjectRecord, ResourceLifecycle, SqliteStateStore,
};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const POSTGRES_17_IMAGE: &str = concat!(
    "postgres@sha256:",
    "ebba4f4de37f08f138f97c1443c987a435e783177afedcc4aaf2da1930fbc37a"
);
const POSTGRES_IMAGE: &str = concat!(
    "postgres@sha256:",
    "9a8afca54e7861fd90fab5fdf4c42477a6b1cb7d293595148e674e0a3181de15"
);

#[test]
#[ignore = "CI owns live incompatible PostgreSQL profile acceptance"]
fn live_docker_engine_incompatible_postgres_majors_use_separate_instances() {
    let socket = std::env::var_os("STACKCTL_ENGINE_SOCKET")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/var/run/docker.sock"));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must follow the Unix epoch")
        .as_nanos();
    let installation_id = format!("ci-{}-{nonce}-mixed-postgres", std::process::id());
    let network_name = format!("stackctl-{installation_id}");
    let state_directory = std::env::temp_dir().join(format!("stackctl-{installation_id}"));
    let platform = match std::env::consts::ARCH {
        "aarch64" => "linux/arm64",
        "x86_64" => "linux/amd64",
        architecture => panic!("unsupported PostgreSQL acceptance architecture '{architecture}'"),
    };
    let sources = [
        postgres_source("bill17", "17", POSTGRES_17_IMAGE),
        postgres_source("shop18", "18", POSTGRES_IMAGE),
    ];
    let registry = plan_project_registry(&sources).expect("plan mixed PostgreSQL registry");
    let execution = crate::control_plane::resolve_execution_plan(&registry)
        .expect("resolve mixed PostgreSQL execution");
    let shared = resolve_execution_shared_instances(&execution, platform)
        .expect("resolve mixed PostgreSQL shared demand");
    let mut planned_profiles = shared
        .iter()
        .map(|plan| {
            (
                plan.profile().implementation().to_owned(),
                plan.profile().major_version().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    planned_profiles.sort();
    assert_eq!(
        planned_profiles,
        vec![
            ("postgresql".to_owned(), "17".to_owned()),
            ("postgresql".to_owned(), "18".to_owned()),
        ],
        "the human-readable compatibility profiles must explain the split"
    );
    assert_eq!(shared.len(), 2);
    assert_ne!(shared[0].fingerprint(), shared[1].fingerprint());

    std::fs::create_dir(&state_directory).expect("create mixed PostgreSQL acceptance state");
    let mut store = SqliteStateStore::open(&state_directory.join("state.sqlite3"))
        .expect("open mixed PostgreSQL acceptance state store");
    let prepared = prepare_postgres_shared_instances(
        &mut store,
        &shared,
        &SequentialCredentialEntropy::new(0x31),
        PostgresPreparationOptions {
            installation_id: &installation_id,
            network_name: &network_name,
            schema_version: 8,
        },
    )
    .expect("prepare mixed PostgreSQL instances");
    let authorized_volumes = prepared
        .iter()
        .map(|instance| {
            instance
                .instance()
                .volume()
                .expect("persistent mixed PostgreSQL volume")
                .name()
                .to_owned()
        })
        .collect::<Vec<_>>();
    let network_metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.clone(),
        kind: ResourceKind::Network,
        project_id: None,
        compatibility_fingerprint: format!("sha256:{}", "a".repeat(64)),
        schema_version: 8,
        desired_revision: format!("sha256:{}", "b".repeat(64)),
        retention: RetentionClass::Persistent,
    })
    .expect("build mixed PostgreSQL network metadata");
    let network_request = NetworkCreateOptions::new(&network_name, network_metadata)
        .expect("build mixed PostgreSQL network request");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build mixed PostgreSQL acceptance runtime");
    let mut engine = runtime
        .block_on(BollardEngineAdapter::connect_unix(&socket))
        .expect("negotiate the selected Docker Engine API");

    let acceptance = runtime.block_on(async {
        for image in [POSTGRES_17_IMAGE, POSTGRES_IMAGE] {
            engine
                .ensure_image(
                    &ImmutableImageReference::new(image)
                        .expect("build mixed PostgreSQL image reference"),
                )
                .await?;
        }
        engine.create_network(&network_request).await?;
        for instance in &prepared {
            let result =
                reconcile_prepared_postgres_instance(&mut engine, instance, &installation_id, 8)
                    .await
                    .map_err(|error| EngineError::Backend {
                        detail: error.to_string(),
                    })?;
            if !result.logical_resource_drifts().is_empty() {
                return Err(EngineError::Backend {
                    detail: format!(
                        "mixed PostgreSQL convergence reported drift: {:?}",
                        result.logical_resource_drifts()
                    ),
                });
            }
        }
        let containers = owned_shared_containers(&engine, &installation_id).await;
        let mut observed_profiles = containers
            .iter()
            .map(|container| {
                (
                    container
                        .metadata()
                        .compatibility_implementation()
                        .expect("shared PostgreSQL implementation label")
                        .to_owned(),
                    container
                        .metadata()
                        .compatibility_major_version()
                        .expect("shared PostgreSQL major-version label")
                        .to_owned(),
                )
            })
            .collect::<Vec<_>>();
        observed_profiles.sort();
        if observed_profiles != planned_profiles {
            return Err(EngineError::Backend {
                detail: format!(
                    "mixed PostgreSQL Engine profiles {observed_profiles:?} did not match plans \
                     {planned_profiles:?}"
                ),
            });
        }
        if containers.len() != 2 || containers[0].id() == containers[1].id() {
            return Err(EngineError::Backend {
                detail: "incompatible PostgreSQL profiles did not produce two containers"
                    .to_owned(),
            });
        }

        Ok::<_, EngineError>(())
    });
    runtime
        .block_on(delete_owned_installation_resources(
            &mut engine,
            InstallationResourceDeletionOptions {
                installation_id: &installation_id,
                schema_version: 8,
                authorized_persistent_volumes: &authorized_volumes,
            },
        ))
        .expect("delete mixed PostgreSQL acceptance resources");
    acceptance.expect("verify incompatible PostgreSQL Engine instances");

    drop(store);
    std::fs::remove_dir_all(&state_directory).expect("remove mixed PostgreSQL acceptance state");
    println!("mixed PostgreSQL profile acceptance passed for {installation_id}");
}

#[test]
#[ignore = "CI owns live shared PostgreSQL isolation acceptance"]
fn live_docker_engine_two_projects_share_one_postgres_with_isolated_databases() {
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
        architecture => panic!("unsupported PostgreSQL acceptance architecture '{architecture}'"),
    };
    let profile = CompatibilityProfile::from_options(CompatibilityFingerprintOptions {
        implementation: "postgresql".to_owned(),
        major_version: "18".to_owned(),
        image_digest: POSTGRES_IMAGE.to_owned(),
        extensions: Vec::new(),
        immutable_settings: BTreeMap::new(),
        persistence: PersistenceMode::Persistent,
        isolation: IsolationCapability::DatabaseAndRole,
        platform_architecture: Some(platform.to_owned()),
    })
    .expect("build PostgreSQL acceptance profile");
    let shared = plan_shared_instances(vec![
        SharedServiceRequest::new("bill", "database", profile.clone()),
        SharedServiceRequest::new("shop", "database", profile),
    ]);
    assert_eq!(shared.len(), 1, "compatible projects must share one plan");
    assert_eq!(shared[0].consumers().len(), 2);
    std::fs::create_dir(&state_directory).expect("create PostgreSQL acceptance state");
    let mut store = SqliteStateStore::open(&state_directory.join("state.sqlite3"))
        .expect("open PostgreSQL acceptance state store");
    let prepared = prepare_postgres_shared_instances(
        &mut store,
        &shared,
        &SequentialCredentialEntropy::new(0x21),
        PostgresPreparationOptions {
            installation_id: &installation_id,
            network_name: &network_name,
            schema_version: 8,
        },
    )
    .expect("prepare shared PostgreSQL acceptance resources");
    let replayed = prepare_postgres_shared_instances(
        &mut store,
        &shared,
        &SequentialCredentialEntropy::new(0x61),
        PostgresPreparationOptions {
            installation_id: &installation_id,
            network_name: &network_name,
            schema_version: 8,
        },
    )
    .expect("replay shared PostgreSQL acceptance preparation");
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
        .find(|project| project.logical().project_id() == "bill")
        .expect("prepared bill PostgreSQL resources");
    let shop = prepared
        .projects()
        .iter()
        .find(|project| project.logical().project_id() == "shop")
        .expect("prepared shop PostgreSQL resources");
    let network_metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.clone(),
        kind: ResourceKind::Network,
        project_id: None,
        compatibility_fingerprint: shared[0].fingerprint().as_str().to_owned(),
        schema_version: 8,
        desired_revision: shared[0].fingerprint().as_str().to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("build PostgreSQL acceptance network metadata");
    let network_request = NetworkCreateOptions::new(&network_name, network_metadata)
        .expect("build PostgreSQL acceptance network request");
    let image = ImmutableImageReference::new(POSTGRES_IMAGE)
        .expect("build immutable PostgreSQL acceptance image reference");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build PostgreSQL acceptance runtime");
    let mut engine = runtime
        .block_on(BollardEngineAdapter::connect_unix(&socket))
        .expect("negotiate the selected Docker Engine API");

    runtime.block_on(async {
        engine
            .ensure_image(&image)
            .await
            .expect("resolve immutable PostgreSQL acceptance image");
        engine
            .create_network(&network_request)
            .await
            .expect("create private PostgreSQL acceptance network");
        let first =
            reconcile_prepared_postgres_instance(&mut engine, prepared, &installation_id, 8)
                .await
                .expect("converge shared PostgreSQL for two projects");
        assert!(
            first.logical_resource_drifts().is_empty(),
            "initial PostgreSQL convergence reported drift: {:?}",
            first.logical_resource_drifts()
        );
        assert_eq!(first.logical_resources().len(), 2);
        let container = owned_shared_container(&engine, &installation_id).await;

        let bill_output = postgres_command(
            &engine,
            &container,
            bill.credential().username(),
            bill.credential().secret(),
            bill.logical().database_name(),
            "CREATE TABLE IF NOT EXISTS stackctl_acceptance(value text); \
             TRUNCATE stackctl_acceptance; \
             INSERT INTO stackctl_acceptance VALUES ('bill-value'); \
             SELECT value FROM stackctl_acceptance;",
        )
        .await
        .expect("write and read bill PostgreSQL database");
        assert_eq!(bill_output, b"bill-value\n");
        let shop_output = postgres_command(
            &engine,
            &container,
            shop.credential().username(),
            shop.credential().secret(),
            shop.logical().database_name(),
            "CREATE TABLE IF NOT EXISTS stackctl_acceptance(value text); \
             TRUNCATE stackctl_acceptance; \
             INSERT INTO stackctl_acceptance VALUES ('shop-value'); \
             SELECT value FROM stackctl_acceptance;",
        )
        .await
        .expect("write and read shop PostgreSQL database");
        assert_eq!(shop_output, b"shop-value\n");
        assert!(matches!(
            postgres_command(
                &engine,
                &container,
                shop.credential().username(),
                shop.credential().secret(),
                bill.logical().database_name(),
                "SELECT 1;",
            )
            .await,
            Err(EngineError::ContainerExit { .. })
        ));

        let source_logical = first
            .logical_resources()
            .iter()
            .find(|logical| logical.project_id() == "bill")
            .expect("find active bill PostgreSQL logical resource");
        let backup_root = state_directory.join("backups");
        let backup = backup_postgres_database(
            &engine,
            &container,
            &PostgresBackupOptions {
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
        .expect("back up live bill PostgreSQL database");
        postgres_command(
            &engine,
            &container,
            bill.credential().username(),
            bill.credential().secret(),
            bill.logical().database_name(),
            "UPDATE stackctl_acceptance SET value = 'mutated-after-backup';",
        )
        .await
        .expect("mutate bill PostgreSQL source after backup");
        let backup_checkpoint = MigrationRecord::new(MigrationRecordOptions {
            migration_id: "bill-database-recovery".to_owned(),
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
        .expect("record live PostgreSQL backup checkpoint");
        let target = reconcile_postgres_migration_target(
            &mut store,
            &mut engine,
            &shared[0],
            &SequentialCredentialEntropy::new(0x71),
            PostgresMigrationPreparationOptions {
                migration_id: backup_checkpoint.migration_id(),
                project_id: "bill",
                installation_id: &installation_id,
                network_name: &network_name,
                schema_version: 8,
                desired_revision: source_logical.desired_revision(),
            },
        )
        .await
        .expect("reconcile isolated PostgreSQL recovery target");
        let target_resources = plan_postgres_project_resources(
            "bill",
            "database",
            target.plan(),
            CredentialSecret::new(bill.credential().secret().to_owned()),
        )
        .expect("plan PostgreSQL recovery target resources");
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
                .expect("plan PostgreSQL recovery cutover");
        let rollback =
            MigrationRollbackPlan::new(project, bill.environment().clone(), vec![retained_target])
                .expect("plan PostgreSQL recovery rollback");
        {
            let mut retirement = EnginePostgresSourceRetirement::new(
                &engine,
                PostgresSourceRetirementOptions {
                    source_container: &container,
                    administrator: prepared.instance().bootstrap_credential(),
                    source_credential: bill.credential(),
                    source_environment: bill.environment(),
                    installation_id: &installation_id,
                    timeout: Duration::from_secs(30),
                },
            )
            .expect("prepare PostgreSQL source retirement boundary");
            let mut operations = PostgresMigrationOperations::new(
                &engine,
                &mut retirement,
                PostgresMigrationOperationsOptions {
                    source_container: &container,
                    target_container: target.container(),
                    source_logical_resource: source_logical,
                    target_logical_resource: &target_logical,
                    source_credential: bill.credential(),
                    target_credential: target_resources.credential(),
                    target_plan: target_resources.logical(),
                    administrator: target.bootstrap_credential(),
                    installation_id: &installation_id,
                    source_database_name: bill.logical().database_name(),
                    backup_root: &backup_root,
                    operation_unix_seconds: 11_700,
                    timeout: Duration::from_secs(30),
                    cutover,
                    rollback,
                },
            )
            .expect("prepare live PostgreSQL migration operations");
            let target_plan = operations
                .provision_target(&backup_checkpoint)
                .await
                .expect("provision isolated PostgreSQL recovery database");
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
            .expect("record live PostgreSQL target checkpoint");
            operations
                .restore(
                    &restore_checkpoint,
                    backup.reference(),
                    target_plan.target_resource_id(),
                )
                .await
                .expect("restore verified PostgreSQL backup into isolated target");
            assert_eq!(
                postgres_command(
                    &engine,
                    target.container(),
                    target_resources.credential().username(),
                    target_resources.credential().secret(),
                    target_plan.target_resource_id(),
                    "SELECT value FROM stackctl_acceptance;",
                )
                .await
                .expect("read restored PostgreSQL target"),
                b"bill-value\n",
                "target restore must reproduce the verified point-in-time backup"
            );
            assert_eq!(
                postgres_command(
                    &engine,
                    &container,
                    shop.credential().username(),
                    shop.credential().secret(),
                    shop.logical().database_name(),
                    "SELECT value FROM stackctl_acceptance;",
                )
                .await
                .expect("read sibling database after PostgreSQL restore"),
                b"shop-value\n"
            );
        }

        let second =
            reconcile_prepared_postgres_instance(&mut engine, prepared, &installation_id, 8)
                .await
                .expect("reconcile unchanged shared PostgreSQL instance");
        assert!(second.logical_resource_drifts().is_empty());
        let replayed_container = owned_shared_container(&engine, &installation_id).await;
        assert_eq!(replayed_container.id(), container.id());

        let bill_logical = first
            .logical_resources()
            .iter()
            .find(|logical| logical.project_id() == "bill")
            .map(orphaned_logical_resource)
            .expect("find bill logical PostgreSQL resource");
        let credentials = [
            disabled_credential(bill.credential()),
            prepared.instance().bootstrap_credential().clone(),
        ];
        let observed = engine
            .discover_managed()
            .await
            .expect("discover PostgreSQL lifecycle acceptance resources");
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
        .expect("revoke removed bill PostgreSQL access");
        assert_eq!(revoked, 1);
        assert!(matches!(
            postgres_command(
                &engine,
                &container,
                bill.credential().username(),
                bill.credential().secret(),
                bill.logical().database_name(),
                "SELECT value FROM stackctl_acceptance;",
            )
            .await,
            Err(EngineError::ContainerExit { .. })
        ));
        assert_eq!(
            postgres_command(
                &engine,
                &container,
                shop.credential().username(),
                shop.credential().secret(),
                shop.logical().database_name(),
                "SELECT value FROM stackctl_acceptance;",
            )
            .await
            .expect("read shop PostgreSQL data after bill removal"),
            b"shop-value\n"
        );
        assert_eq!(
            postgres_command(
                &engine,
                &container,
                prepared.instance().bootstrap_credential().username(),
                prepared.instance().bootstrap_credential().secret(),
                bill.logical().database_name(),
                "SELECT value FROM stackctl_acceptance;",
            )
            .await
            .expect("verify retained bill PostgreSQL data as administrator"),
            b"mutated-after-backup\n",
            "isolated recovery must not mutate the retained source database"
        );

        let restored =
            reconcile_prepared_postgres_instance(&mut engine, prepared, &installation_id, 8)
                .await
                .expect("restore bill PostgreSQL access from active configuration");
        assert!(restored.logical_resource_drifts().is_empty());
        assert_eq!(
            postgres_command(
                &engine,
                &container,
                bill.credential().username(),
                bill.credential().secret(),
                bill.logical().database_name(),
                "SELECT value FROM stackctl_acceptance;",
            )
            .await
            .expect("read restored bill PostgreSQL data"),
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
                        .expect("persistent PostgreSQL volume")
                        .name()
                        .to_owned(),
                    target.volume().name().to_owned(),
                ],
            },
        )
        .await
        .expect("delete shared PostgreSQL acceptance resources");
    });

    drop(store);
    std::fs::remove_dir_all(&state_directory).expect("remove PostgreSQL acceptance state");
    println!("shared PostgreSQL isolation acceptance passed for {installation_id}");
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
        .expect("discover shared PostgreSQL acceptance resources")
        .into_iter()
        .filter_map(|observed| reconstruct_owned_container(&observed, installation_id, 8).ok())
        .find(|owned| owned.metadata().kind() == ResourceKind::SharedService)
        .expect("discover one owned shared PostgreSQL container")
}

async fn owned_shared_containers(
    engine: &BollardEngineAdapter,
    installation_id: &str,
) -> Vec<OwnedContainer> {
    engine
        .discover_managed()
        .await
        .expect("discover mixed PostgreSQL acceptance resources")
        .into_iter()
        .filter_map(|observed| reconstruct_owned_container(&observed, installation_id, 8).ok())
        .filter(|owned| owned.metadata().kind() == ResourceKind::SharedService)
        .collect()
}

fn postgres_source(project: &str, version: &str, image: &str) -> ProjectSource {
    ProjectSource::new(
        std::path::PathBuf::from(format!("/work/{project}")),
        std::path::PathBuf::from(format!("/work/{project}/.stackctl.yaml")),
        format!(
            concat!(
                "schema_version: 8\nproject: {project}\nservices:\n",
                "  database:\n    preset: postgres\n    version: '{version}'\n",
                "    image: {image}\n"
            ),
            project = project,
            version = version,
            image = image,
        ),
    )
}

async fn postgres_command(
    engine: &BollardEngineAdapter,
    container: &OwnedContainer,
    username: &str,
    password: &str,
    database: &str,
    sql: &str,
) -> Result<Vec<u8>, EngineError> {
    let request = CommandRequest::new(
        vec![
            "psql".to_owned(),
            "--no-psqlrc".to_owned(),
            "--set=ON_ERROR_STOP=1".to_owned(),
            "--quiet".to_owned(),
            "--tuples-only".to_owned(),
            "--no-align".to_owned(),
            format!("--username={username}"),
            format!("--dbname={database}"),
            format!("--command={sql}"),
        ],
        BTreeMap::from([("PGPASSWORD".to_owned(), password.to_owned())]),
        None,
    )?;
    let options = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "exercise PostgreSQL tenant isolation",
        Duration::from_secs(15),
    )?;

    run_attached_command_capture(engine, container, &options).await
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
