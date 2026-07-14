use super::{
    BackupArtifactManifest, BackupResourceIdentity, BackupVerificationError, DataLifecycleStrategy,
    DataLifecycleStrategyError, DeletionDecision, InstallationDeletionPlan,
    InstallationDeletionPlanOptions, LogicalPrunePlan, LogicalPrunePlanOptions,
    MinioLogicalPruneOptions, MongoDbLogicalPruneOptions, MySqlLogicalPruneOptions,
    PostgresLogicalPrunePlan, PostgresLogicalPrunePlanOptions, PruneAuthorization,
    RabbitMqLogicalPruneOptions, RedisLogicalPruneOptions, RestoreTarget, RestoreTargetError,
    SqlServerLogicalPruneOptions, evaluate_deletion, open_stored_backup_artifact,
    prune_minio_logical_resource, prune_mongodb_logical_resource, prune_mysql_logical_resource,
    prune_rabbitmq_logical_resource, prune_redis_logical_resource,
    prune_sql_server_logical_resource, resolve_data_lifecycle_strategy, restore_verified_backup,
    store_backup_artifact, store_backup_artifact_for_identity,
    store_backup_artifact_from_async_reader, store_backup_artifact_from_reader,
    verify_backup_artifact, verify_recovery_point_artifact, verify_stored_backup_artifact,
};

#[test]
fn logical_data_kinds_select_explicit_lifecycle_strategies() {
    for (kind, expected) in [
        (
            "postgres_database_and_role",
            DataLifecycleStrategy::PostgreSqlLogical,
        ),
        ("mysql_database", DataLifecycleStrategy::MySqlLogical),
        ("mariadb_database", DataLifecycleStrategy::MySqlLogical),
        ("mongodb_database", DataLifecycleStrategy::MongoDbLogical),
        ("sqlserver_database", DataLifecycleStrategy::SqlServerNative),
        (
            "rabbitmq_vhost_user",
            DataLifecycleStrategy::RabbitMqDefinitions,
        ),
        (
            "minio_bucket_policy",
            DataLifecycleStrategy::ObjectStoreBucketExport,
        ),
        (
            "redis_acl_prefix",
            DataLifecycleStrategy::SharedKeyValueSnapshot,
        ),
    ] {
        assert_eq!(
            resolve_data_lifecycle_strategy(&logical_resource_of_kind(kind)),
            Ok(expected),
        );
    }
}

#[test]
fn lifecycle_strategy_resolution_fails_loudly_for_non_data_and_unknown_kinds() {
    assert_eq!(
        resolve_data_lifecycle_strategy(&logical_resource_of_kind("mailpit_smtp_identity")),
        Err(DataLifecycleStrategyError::NonAuthoritative {
            kind: "mailpit_smtp_identity".to_owned(),
        }),
    );
    assert_eq!(
        resolve_data_lifecycle_strategy(&logical_resource_of_kind("future_database")),
        Err(DataLifecycleStrategyError::UnknownKind {
            kind: "future_database".to_owned(),
        }),
    );
}
use crate::control_plane::engine::{
    CommandExecutionId, CommandExecutor, CommandRequest, CommandSession, CommandStatus,
    ContainerId, ContainerLogStream, EngineFuture, OwnedContainer,
};
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
    LogicalResourceRecordOptions, RecoveryPointRecord, RecoveryPointRecordOptions,
    ResourceLifecycle, ResourceRecord, ResourceRecordOptions, ResourceRetention,
};
use futures_util::stream;
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};
use tokio::io::AsyncReadExt;

#[derive(Clone, Default)]
struct RecordingPruneExecutor {
    arguments: Arc<Mutex<Vec<String>>>,
    argument_calls: Arc<Mutex<Vec<Vec<String>>>>,
    environment: Arc<Mutex<BTreeMap<String, String>>>,
    stdin: Arc<Mutex<Vec<u8>>>,
    outputs: Arc<Mutex<VecDeque<Vec<u8>>>>,
}

impl RecordingPruneExecutor {
    fn with_outputs(outputs: Vec<Vec<u8>>) -> Self {
        Self {
            outputs: Arc::new(Mutex::new(outputs.into())),
            ..Self::default()
        }
    }

    fn arguments(&self) -> Vec<String> {
        self.arguments.lock().expect("arguments lock").clone()
    }

    fn environment(&self) -> BTreeMap<String, String> {
        self.environment.lock().expect("environment lock").clone()
    }

    fn argument_calls(&self) -> Vec<Vec<String>> {
        self.argument_calls
            .lock()
            .expect("argument calls lock")
            .clone()
    }

    fn stdin(&self) -> Vec<u8> {
        self.stdin.lock().expect("stdin lock").clone()
    }
}

impl CommandExecutor for RecordingPruneExecutor {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        *self.arguments.lock().expect("arguments lock") = request.arguments().to_vec();
        self.argument_calls
            .lock()
            .expect("argument calls lock")
            .push(request.arguments().to_vec());
        *self.environment.lock().expect("environment lock") = request.environment().clone();
        let stdin = Arc::clone(&self.stdin);
        let output_bytes = self
            .outputs
            .lock()
            .expect("command outputs lock")
            .pop_front()
            .unwrap_or_default();
        let container_id = container.id().clone();

        Box::pin(async move {
            let (writer, mut reader) = tokio::io::duplex(16 * 1024);
            tokio::spawn(async move {
                let mut bytes = Vec::new();
                reader
                    .read_to_end(&mut bytes)
                    .await
                    .expect("read prune stdin");
                *stdin.lock().expect("stdin lock") = bytes;
            });
            let output: ContainerLogStream<'static> = if output_bytes.is_empty() {
                Box::pin(stream::empty())
            } else {
                Box::pin(stream::once(async move {
                    Ok(crate::control_plane::engine::LogChunk::stdout(output_bytes))
                }))
            };

            Ok(CommandSession::new(
                CommandExecutionId::new("prune-exec"),
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
        Box::pin(async {
            tokio::task::yield_now().await;
            Ok(CommandStatus::Exited(0))
        })
    }
}

#[test]
fn mysql_logical_prune_uses_exact_idempotent_schema_and_user_deletion() {
    use crate::control_plane::engine::{
        ContainerId, ManagedResourceMetadata, ManagedResourceMetadataOptions, ObservedContainer,
        ResourceKind, RetentionClass, reconstruct_owned_container,
    };
    use crate::control_plane::shared_infrastructure::MySqlFlavor;
    use std::time::Duration;

    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::SharedService,
        project_id: None,
        compatibility_fingerprint: "sha256:mysql-8".to_owned(),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("MySQL metadata");
    let observed = ObservedContainer::new(ContainerId::new("mysql-8"), metadata.labels());
    let container =
        reconstruct_owned_container(&observed, "install-1", 8).expect("owned MySQL container");
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "stackctl_bill_database".to_owned(),
        shared_resource_id: "mysql-8".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "mysql_database".to_owned(),
        compatibility_fingerprint: "sha256:mysql-8".to_owned(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(40_000),
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/mysql".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "st_bill_database".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Disabled,
    });
    let administrator = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "shared/mysql-8/bootstrap".to_owned(),
        project_id: None,
        service_id: "mysql".to_owned(),
        username: "root".to_owned(),
        secret: "administrator-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let executor = RecordingPruneExecutor::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    runtime
        .block_on(prune_mysql_logical_resource(
            &executor,
            MySqlLogicalPruneOptions {
                installation_id: "install-1",
                flavor: MySqlFlavor::MySql,
                container: &container,
                logical_resource: &logical,
                credential: &credential,
                administrator: &administrator,
                timeout: Duration::from_secs(30),
            },
        ))
        .expect("prune MySQL tenant");

    assert_eq!(executor.arguments()[0], "mysql");
    assert_eq!(
        executor.environment().get("MYSQL_PWD"),
        Some(&"administrator-secret".to_owned())
    );
    let sql = String::from_utf8(executor.stdin()).expect("MySQL prune SQL");
    assert!(sql.contains("DROP DATABASE IF EXISTS `stackctl_bill_database`"));
    assert!(sql.contains("DROP USER IF EXISTS 'st_bill_database'@'%'"));
    assert!(!sql.contains("project-secret"));
}

#[test]
fn mongodb_logical_prune_uses_exact_idempotent_database_and_user_deletion() {
    use crate::control_plane::engine::{
        ContainerId, ManagedResourceMetadata, ManagedResourceMetadataOptions, ObservedContainer,
        ResourceKind, RetentionClass, reconstruct_owned_container,
    };
    use std::time::Duration;

    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::SharedService,
        project_id: None,
        compatibility_fingerprint: "sha256:mongodb-8".to_owned(),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("MongoDB metadata");
    let observed = ObservedContainer::new(ContainerId::new("mongodb-8"), metadata.labels());
    let container =
        reconstruct_owned_container(&observed, "install-1", 8).expect("owned MongoDB container");
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "stackctl_bill_database".to_owned(),
        shared_resource_id: "mongodb-8".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "mongodb_database".to_owned(),
        compatibility_fingerprint: "sha256:mongodb-8".to_owned(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(40_000),
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/mongodb".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "st_bill_database".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Disabled,
    });
    let administrator = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "shared/mongodb-8/bootstrap".to_owned(),
        project_id: None,
        service_id: "mongodb".to_owned(),
        username: "stackctl_admin".to_owned(),
        secret: "administrator-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let executor = RecordingPruneExecutor::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    runtime
        .block_on(prune_mongodb_logical_resource(
            &executor,
            MongoDbLogicalPruneOptions {
                installation_id: "install-1",
                container: &container,
                logical_resource: &logical,
                credential: &credential,
                administrator: &administrator,
                timeout: Duration::from_secs(30),
            },
        ))
        .expect("prune MongoDB tenant");

    assert_eq!(executor.arguments(), ["mongosh", "--quiet", "--nodb"]);
    assert!(executor.environment().is_empty());
    let script = String::from_utf8(executor.stdin()).expect("MongoDB prune script");
    assert!(script.contains("getUser(\"st_bill_database\")"));
    assert!(script.contains("dropUser(\"st_bill_database\")"));
    assert!(script.contains("dropDatabase()"));
    assert!(script.contains("administrator-secret"));
    assert!(!script.contains("project-secret"));
}

#[test]
fn sql_server_logical_prune_uses_exact_idempotent_database_and_login_deletion() {
    use crate::control_plane::engine::{
        ContainerId, ManagedResourceMetadata, ManagedResourceMetadataOptions, ObservedContainer,
        ResourceKind, RetentionClass, reconstruct_owned_container,
    };
    use std::time::Duration;

    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::SharedService,
        project_id: None,
        compatibility_fingerprint: "sha256:sqlserver-2022".to_owned(),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("SQL Server metadata");
    let observed = ObservedContainer::new(ContainerId::new("sqlserver-2022"), metadata.labels());
    let container =
        reconstruct_owned_container(&observed, "install-1", 8).expect("owned SQL Server container");
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "stackctl_bill_database".to_owned(),
        shared_resource_id: "sqlserver-2022".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "sqlserver_database".to_owned(),
        compatibility_fingerprint: "sha256:sqlserver-2022".to_owned(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(40_000),
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/sqlserver".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "st_bill_database".to_owned(),
        secret: "ProjectSecret1".to_owned(),
        lifecycle: CredentialLifecycle::Disabled,
    });
    let administrator = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "shared/sqlserver-2022/bootstrap".to_owned(),
        project_id: None,
        service_id: "sqlserver".to_owned(),
        username: "sa".to_owned(),
        secret: "AdministratorSecret1".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let executor = RecordingPruneExecutor::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    runtime
        .block_on(prune_sql_server_logical_resource(
            &executor,
            SqlServerLogicalPruneOptions {
                installation_id: "install-1",
                container: &container,
                logical_resource: &logical,
                credential: &credential,
                administrator: &administrator,
                timeout: Duration::from_secs(60),
            },
        ))
        .expect("prune SQL Server tenant");

    assert_eq!(executor.arguments()[0], "/opt/mssql-tools18/bin/sqlcmd");
    assert_eq!(
        executor.environment().get("SQLCMDPASSWORD"),
        Some(&"AdministratorSecret1".to_owned())
    );
    let sql = String::from_utf8(executor.stdin()).expect("SQL Server prune SQL");
    assert!(sql.contains("IF DB_ID(N'stackctl_bill_database') IS NOT NULL"));
    assert!(sql.contains("DROP DATABASE [stackctl_bill_database]"));
    assert!(sql.contains("DROP LOGIN [st_bill_database]"));
    assert!(!sql.contains("ProjectSecret1"));
}

#[test]
fn rabbitmq_logical_prune_revokes_user_then_deletes_only_the_exact_vhost() {
    use crate::control_plane::engine::{
        ContainerId, ManagedResourceMetadata, ManagedResourceMetadataOptions, ObservedContainer,
        ResourceKind, RetentionClass, reconstruct_owned_container,
    };
    use std::time::Duration;

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
    let observed = ObservedContainer::new(ContainerId::new("rabbitmq-4"), metadata.labels());
    let container =
        reconstruct_owned_container(&observed, "install-1", 8).expect("owned RabbitMQ container");
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/database/rabbitmq".to_owned(),
        shared_resource_id: "rabbitmq-4".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "rabbitmq_vhost_user".to_owned(),
        compatibility_fingerprint: "sha256:rabbitmq-4".to_owned(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(40_000),
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/rabbitmq".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "st_bill_database".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Disabled,
    });
    let executor = RecordingPruneExecutor::with_outputs(vec![
        b"st_bill_database\nother_user\n".to_vec(),
        Vec::new(),
        b"/\nstackctl_bill_database\nother_vhost\n".to_vec(),
        Vec::new(),
    ]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    runtime
        .block_on(prune_rabbitmq_logical_resource(
            &executor,
            RabbitMqLogicalPruneOptions {
                installation_id: "install-1",
                container: &container,
                logical_resource: &logical,
                credential: &credential,
                timeout: Duration::from_secs(30),
            },
        ))
        .expect("prune RabbitMQ tenant");

    let calls = executor.argument_calls();
    assert_eq!(calls[0][..2], ["rabbitmqctl", "list_users"]);
    assert_eq!(calls[1], ["rabbitmqctl", "delete_user", "st_bill_database"]);
    assert_eq!(calls[2][..2], ["rabbitmqctl", "list_vhosts"]);
    assert_eq!(
        calls[3],
        ["rabbitmqctl", "delete_vhost", "stackctl_bill_database"]
    );
    assert!(calls.iter().flatten().all(|value| value != "other_vhost"));
    assert!(executor.environment().is_empty());
    assert!(!String::from_utf8_lossy(&executor.stdin()).contains("project-secret"));

    let replay = RecordingPruneExecutor::with_outputs(vec![Vec::new(), Vec::new()]);
    runtime
        .block_on(prune_rabbitmq_logical_resource(
            &replay,
            RabbitMqLogicalPruneOptions {
                installation_id: "install-1",
                container: &container,
                logical_resource: &logical,
                credential: &credential,
                timeout: Duration::from_secs(30),
            },
        ))
        .expect("replay already deleted RabbitMQ tenant");
    assert_eq!(replay.argument_calls().len(), 2);
}

#[test]
fn minio_logical_prune_deletes_only_exact_bucket_identity_and_policy() {
    use crate::control_plane::engine::{
        ContainerId, ManagedResourceMetadata, ManagedResourceMetadataOptions, ObservedContainer,
        ResourceKind, RetentionClass, reconstruct_owned_container,
    };
    use std::time::Duration;

    let fingerprint = format!("sha256:{}", "a".repeat(64));
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::SharedService,
        project_id: None,
        compatibility_fingerprint: fingerprint.clone(),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("MinIO metadata");
    let observed = ObservedContainer::new(ContainerId::new("minio-1"), metadata.labels());
    let container =
        reconstruct_owned_container(&observed, "install-1", 8).expect("owned MinIO container");
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/files/object-store".to_owned(),
        shared_resource_id: "minio-1".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "files".to_owned(),
        kind: "minio_bucket_policy".to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(40_000),
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/files/object-store".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "files".to_owned(),
        username: "st_bill_files".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Disabled,
    });
    let administrator = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!(
            "shared/{}/minio-root",
            fingerprint.strip_prefix("sha256:").expect("fingerprint")
        ),
        project_id: None,
        service_id: "minio".to_owned(),
        username: "stackctl_admin".to_owned(),
        secret: "administrator-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let executor = RecordingPruneExecutor::with_outputs(vec![
        br#"{"status":"success","accessKey":"st_bill_files"}"#.to_vec(),
        Vec::new(),
        br#"{"status":"success","type":"folder","key":"stackctl-bill-files/"}"#.to_vec(),
        Vec::new(),
        br#"{"status":"success","policy":"stackctl-bill-files"}"#.to_vec(),
        Vec::new(),
    ]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    runtime
        .block_on(prune_minio_logical_resource(
            &executor,
            MinioLogicalPruneOptions {
                installation_id: "install-1",
                container: &container,
                logical_resource: &logical,
                credential: &credential,
                administrator: &administrator,
                timeout: Duration::from_secs(30),
            },
        ))
        .expect("prune MinIO tenant");

    let calls = executor.argument_calls();
    assert_eq!(calls[0][..4], ["mc", "admin", "user", "list"]);
    assert_eq!(
        calls[1],
        ["mc", "admin", "user", "remove", "stackctl", "st_bill_files"]
    );
    assert_eq!(calls[2][..2], ["mc", "ls"]);
    assert_eq!(
        calls[3],
        ["mc", "rb", "--force", "stackctl/stackctl-bill-files"]
    );
    assert_eq!(calls[4][..4], ["mc", "admin", "policy", "list"]);
    assert_eq!(
        calls[5],
        [
            "mc",
            "admin",
            "policy",
            "remove",
            "stackctl",
            "stackctl-bill-files"
        ]
    );
    assert!(executor.environment()["MC_HOST_stackctl"].contains("administrator-secret"));
    assert!(!format!("{calls:?}").contains("project-secret"));

    let replay = RecordingPruneExecutor::with_outputs(vec![Vec::new(), Vec::new(), Vec::new()]);
    runtime
        .block_on(prune_minio_logical_resource(
            &replay,
            MinioLogicalPruneOptions {
                installation_id: "install-1",
                container: &container,
                logical_resource: &logical,
                credential: &credential,
                administrator: &administrator,
                timeout: Duration::from_secs(30),
            },
        ))
        .expect("replay missing MinIO tenant prune");
    assert_eq!(replay.argument_calls().len(), 3);
}

#[test]
fn redis_logical_prune_revokes_the_user_then_deletes_only_the_exact_prefix() {
    use crate::control_plane::engine::{
        ContainerId, ManagedResourceMetadata, ManagedResourceMetadataOptions, ObservedContainer,
        ResourceKind, RetentionClass, reconstruct_owned_container,
    };
    use crate::control_plane::shared_infrastructure::RedisFlavor;
    use std::time::Duration;

    let fingerprint = format!("sha256:{}", "a".repeat(64));
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::SharedService,
        project_id: None,
        compatibility_fingerprint: fingerprint.clone(),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("Redis metadata");
    let observed = ObservedContainer::new(ContainerId::new("redis-1"), metadata.labels());
    let container =
        reconstruct_owned_container(&observed, "install-1", 8).expect("owned Redis container");
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/cache/redis".to_owned(),
        shared_resource_id: "redis-1".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "cache".to_owned(),
        kind: "redis_acl_prefix".to_owned(),
        compatibility_fingerprint: fingerprint.clone(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(40_000),
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/cache/redis".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "cache".to_owned(),
        username: "st_bill_cache".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Disabled,
    });
    let administrator = CredentialRecord::new(CredentialRecordOptions {
        credential_id: format!("shared/{}/redis-bootstrap", "a".repeat(64)),
        project_id: None,
        service_id: "redis".to_owned(),
        username: "stackctl_admin".to_owned(),
        secret: "administrator-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let executor = RecordingPruneExecutor::with_outputs(vec![b"1\n".to_vec(), b"2\n".to_vec()]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    runtime
        .block_on(prune_redis_logical_resource(
            &executor,
            RedisLogicalPruneOptions {
                installation_id: "install-1",
                flavor: RedisFlavor::Redis,
                container: &container,
                logical_resource: &logical,
                credential: &credential,
                administrator: &administrator,
                timeout: Duration::from_secs(30),
            },
        ))
        .expect("prune Redis tenant");

    let calls = executor.argument_calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0][0], "redis-cli");
    assert_eq!(&calls[0][4..], ["ACL", "DELUSER", "st_bill_cache"]);
    assert_eq!(calls[1][4], "EVAL");
    assert!(calls[1][5].contains("SCAN"));
    assert!(calls[1][5].contains("UNLINK"));
    assert_eq!(calls[1].last(), Some(&"stackctl:bill:cache:".to_owned()));
    assert_eq!(
        executor.environment()["REDISCLI_AUTH"],
        "administrator-secret"
    );
    assert!(!format!("{calls:?}").contains("project-secret"));

    let replay = RecordingPruneExecutor::with_outputs(vec![b"0\n".to_vec(), b"0\n".to_vec()]);
    runtime
        .block_on(prune_redis_logical_resource(
            &replay,
            RedisLogicalPruneOptions {
                installation_id: "install-1",
                flavor: RedisFlavor::Redis,
                container: &container,
                logical_resource: &logical,
                credential: &credential,
                administrator: &administrator,
                timeout: Duration::from_secs(30),
            },
        ))
        .expect("replay already deleted Redis tenant");
    assert_eq!(replay.argument_calls().len(), 2);
}

#[test]
fn postgres_prune_plan_binds_exact_retained_state_backup_and_confirmation() {
    let logical = prune_logical(ResourceLifecycle::Orphaned, Some(10_000));
    let credential = prune_credential(CredentialLifecycle::Disabled);
    let recovery = prune_recovery_point("backup-42", "a");

    let first = PostgresLogicalPrunePlan::new(PostgresLogicalPrunePlanOptions {
        installation_id: "install-1",
        project_id: "bill",
        service_id: "database",
        recovery_point_id: "backup-42",
        project_registered: false,
        logical_resources: std::slice::from_ref(&logical),
        credentials: std::slice::from_ref(&credential),
        recovery_points: std::slice::from_ref(&recovery),
    })
    .expect("safe prune plan");
    let second = PostgresLogicalPrunePlan::new(PostgresLogicalPrunePlanOptions {
        installation_id: "install-1",
        project_id: "bill",
        service_id: "database",
        recovery_point_id: "backup-42",
        project_registered: false,
        logical_resources: &[logical],
        credentials: &[credential],
        recovery_points: &[recovery],
    })
    .expect("deterministic prune plan");

    assert_eq!(first, second);
    assert_eq!(first.logical_resource_id(), "stackctl_bill_database");
    assert_eq!(first.shared_resource_id(), "postgres-shared-17");
    assert_eq!(first.credential_id(), "bill/database/postgresql");
    assert_eq!(first.recovery_point_id(), "backup-42");
    assert_eq!(first.confirmation_token().len(), 64);
    assert!(
        first
            .confirmation_token()
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );
}

#[test]
fn installation_deletion_plan_selects_latest_exact_recovery_before_freeze() {
    let logical = prune_logical(ResourceLifecycle::Active, None);
    let credential = prune_credential(CredentialLifecycle::Active);
    let older = prune_recovery_point_at("backup-41", "a", 8_000);
    let latest = prune_recovery_point_at("backup-42", "b", 9_000);

    let plan = InstallationDeletionPlan::new(InstallationDeletionPlanOptions {
        installation_id: "install-1",
        logical_resources: std::slice::from_ref(&logical),
        resources: &[],
        credentials: std::slice::from_ref(&credential),
        recovery_points: &[older, latest],
    })
    .expect("complete installation deletion plan");

    let [item] = plan.logical_prunes() else {
        panic!("expected one logical prune");
    };
    assert_eq!(item.project_id(), "bill");
    assert_eq!(item.service_id(), "database");
    assert_eq!(item.recovery_point_id(), "backup-42");
    assert_eq!(plan.confirmation_token().len(), 64);
    assert!(
        plan.confirmation_token()
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );
}

#[test]
fn logical_prune_plan_uses_one_authorization_contract_for_mysql() {
    let logical = mysql_prune_logical();
    let credential = mysql_prune_credential();
    let recovery = mysql_prune_recovery_point();

    let plan = LogicalPrunePlan::new(LogicalPrunePlanOptions {
        installation_id: "install-1",
        project_id: "bill",
        service_id: "database",
        recovery_point_id: "backup-mysql",
        project_registered: false,
        logical_resources: std::slice::from_ref(&logical),
        credentials: std::slice::from_ref(&credential),
        recovery_points: std::slice::from_ref(&recovery),
    })
    .expect("safe MySQL prune plan");

    assert_eq!(plan.strategy(), DataLifecycleStrategy::MySqlLogical);
    assert_eq!(plan.logical_resource_id(), "stackctl_bill_database");
    assert_eq!(plan.credential_id(), "bill/database/mysql");
    assert_eq!(plan.recovery_point_id(), "backup-mysql");
    assert_eq!(plan.confirmation_token().len(), 64);
}

#[test]
fn postgres_prune_plan_refuses_active_ambiguous_or_unverified_state() {
    let orphaned = prune_logical(ResourceLifecycle::Orphaned, Some(10_000));
    let active = prune_logical(ResourceLifecycle::Active, None);
    let disabled = prune_credential(CredentialLifecycle::Disabled);
    let active_credential = prune_credential(CredentialLifecycle::Active);
    let recovery = prune_recovery_point("backup-42", "a");

    for (label, options) in [
        (
            "registered project",
            PostgresLogicalPrunePlanOptions {
                installation_id: "install-1",
                project_id: "bill",
                service_id: "database",
                recovery_point_id: "backup-42",
                project_registered: true,
                logical_resources: std::slice::from_ref(&orphaned),
                credentials: std::slice::from_ref(&disabled),
                recovery_points: std::slice::from_ref(&recovery),
            },
        ),
        (
            "active logical resource",
            PostgresLogicalPrunePlanOptions {
                installation_id: "install-1",
                project_id: "bill",
                service_id: "database",
                recovery_point_id: "backup-42",
                project_registered: false,
                logical_resources: std::slice::from_ref(&active),
                credentials: std::slice::from_ref(&disabled),
                recovery_points: std::slice::from_ref(&recovery),
            },
        ),
        (
            "active credential",
            PostgresLogicalPrunePlanOptions {
                installation_id: "install-1",
                project_id: "bill",
                service_id: "database",
                recovery_point_id: "backup-42",
                project_registered: false,
                logical_resources: std::slice::from_ref(&orphaned),
                credentials: std::slice::from_ref(&active_credential),
                recovery_points: std::slice::from_ref(&recovery),
            },
        ),
        (
            "wrong recovery point",
            PostgresLogicalPrunePlanOptions {
                installation_id: "install-1",
                project_id: "bill",
                service_id: "database",
                recovery_point_id: "backup-other",
                project_registered: false,
                logical_resources: std::slice::from_ref(&orphaned),
                credentials: std::slice::from_ref(&disabled),
                recovery_points: std::slice::from_ref(&recovery),
            },
        ),
    ] {
        let error = PostgresLogicalPrunePlan::new(options).expect_err(label);

        assert!(!error.is_empty());
    }
}

#[test]
fn active_resources_are_never_garbage_collected() {
    let resource = resource(
        ResourceRetention::Disposable,
        ResourceLifecycle::Active,
        None,
    );

    assert_eq!(
        evaluate_deletion(&resource, 10_000, 100, PruneAuthorization::None),
        DeletionDecision::KeepActive
    );
}

#[test]
fn expired_disposable_orphans_are_automatically_deletable() {
    let resource = resource(
        ResourceRetention::Disposable,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );

    assert_eq!(
        evaluate_deletion(&resource, 1_101, 100, PruneAuthorization::None),
        DeletionDecision::DeleteDisposable
    );
}

#[test]
fn unexpired_disposable_orphans_are_retained() {
    let resource = resource(
        ResourceRetention::Disposable,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );

    assert_eq!(
        evaluate_deletion(&resource, 1_099, 100, PruneAuthorization::None),
        DeletionDecision::StopAndRetain
    );
}

#[test]
fn build_caches_follow_the_disposable_retention_window() {
    let resource = resource(
        ResourceRetention::BuildCache,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );

    assert_eq!(
        evaluate_deletion(&resource, 1_100, 100, PruneAuthorization::None),
        DeletionDecision::DeleteDisposable
    );
}

#[test]
fn disposable_resources_without_an_orphan_timestamp_are_retained() {
    let resource = resource(
        ResourceRetention::Disposable,
        ResourceLifecycle::Retained,
        None,
    );

    assert_eq!(
        evaluate_deletion(&resource, 50_000, 100, PruneAuthorization::None),
        DeletionDecision::StopAndRetain
    );
}

#[test]
fn persistent_orphans_require_explicit_prune_and_verified_backup() {
    let resource = resource(
        ResourceRetention::Persistent,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );

    assert_eq!(
        evaluate_deletion(&resource, 50_000, 100, PruneAuthorization::None),
        DeletionDecision::StopAndRetain
    );
    assert_eq!(
        evaluate_deletion(
            &resource,
            50_000,
            100,
            PruneAuthorization::Explicit { backup: None },
        ),
        DeletionDecision::AwaitVerifiedBackup
    );
    let manifest = BackupArtifactManifest::from_artifact(&resource, b"verified backup", 49_000)
        .expect("backup manifest");
    let backup = verify_backup_artifact(&manifest, b"verified backup", 49_500)
        .expect("verified backup evidence");
    assert_eq!(
        evaluate_deletion(
            &resource,
            50_000,
            100,
            PruneAuthorization::Explicit {
                backup: Some(backup),
            },
        ),
        DeletionDecision::DeleteAuthorized
    );
}

#[test]
fn persistent_prune_rejects_tampered_or_wrong_resource_backup_evidence() {
    let resource = resource(
        ResourceRetention::Persistent,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );
    let manifest = BackupArtifactManifest::from_artifact(&resource, b"backup bytes", 40_000)
        .expect("backup manifest");

    let error = verify_backup_artifact(&manifest, b"tampered bytes", 41_000)
        .expect_err("tampered artifact");
    assert_eq!(
        error.to_string(),
        "backup artifact checksum does not match its manifest"
    );

    let other = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "resource-2".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "volume".to_owned(),
        compatibility_fingerprint: "sha256:other-fingerprint".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(1_000),
    });
    let evidence = verify_backup_artifact(&manifest, b"backup bytes", 41_000)
        .expect("valid but wrong evidence");
    assert_eq!(
        evaluate_deletion(
            &other,
            50_000,
            100,
            PruneAuthorization::Explicit {
                backup: Some(evidence),
            },
        ),
        DeletionDecision::AwaitVerifiedBackup
    );
}

#[test]
fn backup_manifest_serializes_portable_resource_identity_and_checksum() {
    let resource = resource(
        ResourceRetention::Persistent,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );
    let manifest = BackupArtifactManifest::from_artifact(&resource, b"backup bytes", 40_000)
        .expect("backup manifest");

    assert_eq!(
        serde_json::to_value(manifest).expect("manifest JSON"),
        serde_json::json!({
            "schema_version": 1,
            "resource_id": "resource-1",
            "installation_id": "install-1",
            "resource_kind": "volume",
            "compatibility_fingerprint": "sha256:fingerprint",
            "artifact_sha256":
                "7171b7ccbaa1ac3767c1e75815c6c5bca6634f141b55b4d1a398ddf2a76b75df",
            "artifact_size_bytes": 12,
            "created_at_unix_seconds": 40_000,
        })
    );
}

#[test]
fn backup_verification_rejects_impossible_timestamps() {
    let resource = resource(
        ResourceRetention::Persistent,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );
    let manifest = BackupArtifactManifest::from_artifact(&resource, b"backup bytes", 40_000)
        .expect("backup manifest");

    let error = verify_backup_artifact(&manifest, b"backup bytes", 39_999)
        .expect_err("verification before creation");
    assert_eq!(
        error.to_string(),
        "backup verification time predates artifact creation"
    );
}

#[cfg(unix)]
#[test]
fn backup_artifacts_are_private_atomic_immutable_and_reread_for_verification() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "stackctl-backup-store-{}-{}",
        std::process::id(),
        50_000
    ));
    let resource = resource(
        ResourceRetention::Persistent,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );

    let stored = store_backup_artifact(&resource, b"recoverable bytes", 40_000, &root)
        .expect("stored backup");
    let repeated = store_backup_artifact(&resource, b"recoverable bytes", 40_000, &root)
        .expect("idempotent backup store");

    assert_eq!(stored, repeated);
    assert_eq!(
        std::fs::read(stored.artifact_file()).expect("artifact bytes"),
        b"recoverable bytes"
    );
    assert_eq!(
        std::fs::metadata(&root)
            .expect("backup root metadata")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    for path in [stored.artifact_file(), stored.manifest_file()] {
        assert_eq!(
            std::fs::metadata(path)
                .expect("backup file metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    let evidence = verify_stored_backup_artifact(&stored, 41_000).expect("stored evidence");
    assert_eq!(
        evaluate_deletion(
            &resource,
            50_000,
            100,
            PruneAuthorization::Explicit {
                backup: Some(evidence),
            },
        ),
        DeletionDecision::DeleteAuthorized
    );

    std::fs::write(stored.artifact_file(), b"tampered").expect("tamper artifact");
    let error = verify_stored_backup_artifact(&stored, 41_000).expect_err("tampered stored backup");
    assert_eq!(
        error.to_string(),
        "backup artifact checksum does not match its manifest"
    );

    std::fs::remove_dir_all(&root).expect("remove backup fixture");
}

#[cfg(unix)]
#[test]
fn backup_store_retains_distinct_recovery_points_for_one_resource() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-backup-history-{}-{}",
        std::process::id(),
        50_001
    ));
    let resource = resource(
        ResourceRetention::Persistent,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );

    let first =
        store_backup_artifact(&resource, b"first backup", 40_000, &root).expect("first backup");
    let second =
        store_backup_artifact(&resource, b"second backup", 41_000, &root).expect("second backup");

    assert_ne!(first, second);
    assert!(first.artifact_file().is_file());
    assert!(second.artifact_file().is_file());

    std::fs::remove_dir_all(&root).expect("remove backup history fixture");
}

#[cfg(unix)]
#[test]
fn backup_store_recovers_an_incomplete_owned_pending_publish() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "stackctl-backup-recovery-{}-{}",
        std::process::id(),
        50_002
    ));
    let resource = resource(
        ResourceRetention::Persistent,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );
    let initially_stored = store_backup_artifact(&resource, b"recoverable bytes", 40_000, &root)
        .expect("initial backup");
    let destination = initially_stored
        .artifact_file()
        .parent()
        .expect("destination directory")
        .to_owned();
    let resource_directory = destination.parent().expect("resource directory");
    let pending = resource_directory.join(".pending-40000");
    std::fs::remove_dir_all(&destination).expect("simulate unpublished backup");
    std::fs::create_dir(&pending).expect("stale pending directory");
    std::fs::set_permissions(&pending, std::fs::Permissions::from_mode(0o700))
        .expect("protect pending directory");
    std::fs::write(pending.join("artifact.bin"), b"partial").expect("partial pending artifact");

    let recovered = store_backup_artifact(&resource, b"recoverable bytes", 40_000, &root)
        .expect("recovered backup publish");

    assert_eq!(recovered, initially_stored);
    assert!(!pending.exists());
    verify_stored_backup_artifact(&recovered, 41_000).expect("verified recovered backup");

    std::fs::remove_dir_all(&root).expect("remove backup recovery fixture");
}

#[cfg(unix)]
#[test]
fn backup_store_refuses_a_symbolic_link_recovery_point() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "stackctl-backup-linked-recovery-point-{}-{}",
        std::process::id(),
        50_004
    ));
    let resource = resource(
        ResourceRetention::Persistent,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );
    let stored = store_backup_artifact(&resource, b"recoverable bytes", 40_000, &root)
        .expect("initial backup");
    let destination = stored
        .artifact_file()
        .parent()
        .expect("recovery point directory")
        .to_owned();
    let victim = root.join("external-recovery-point");
    std::fs::rename(&destination, &victim).expect("move recovery point outside resource");
    symlink(&victim, &destination).expect("create linked recovery point");

    let error = store_backup_artifact(&resource, b"recoverable bytes", 40_000, &root)
        .expect_err("linked recovery point must fail closed");

    assert!(error.to_string().contains("real directory"));
    assert!(
        std::fs::symlink_metadata(&destination)
            .expect("linked recovery point")
            .file_type()
            .is_symlink()
    );

    std::fs::remove_dir_all(&root).expect("remove linked recovery-point fixture");
}

#[cfg(unix)]
#[test]
fn backup_store_streams_large_artifacts_without_requiring_one_byte_buffer() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-backup-stream-{}-{}",
        std::process::id(),
        50_003
    ));
    let resource = resource(
        ResourceRetention::Persistent,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );
    let bytes = vec![b'x'; 256 * 1024 + 17];
    let reader = ChunkedReader::new(&bytes, 37);

    let stored = store_backup_artifact_from_reader(&resource, reader, 42_000, &root)
        .expect("streamed backup");

    assert_eq!(
        std::fs::metadata(stored.artifact_file())
            .expect("artifact metadata")
            .len(),
        u64::try_from(bytes.len()).expect("fixture length")
    );
    verify_stored_backup_artifact(&stored, 42_001).expect("verified streamed backup");

    std::fs::remove_dir_all(&root).expect("remove streamed backup fixture");
}

#[cfg(unix)]
#[test]
fn backup_store_streams_async_artifacts_directly_into_recovery_points() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("backup runtime");
    let root = std::env::temp_dir().join(format!(
        "stackctl-backup-async-stream-{}-{}",
        std::process::id(),
        50_004
    ));
    let resource = resource(
        ResourceRetention::Persistent,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );
    let bytes = vec![b'y'; 256 * 1024 + 29];
    let mut reader = std::io::Cursor::new(bytes.clone());
    let identity = BackupResourceIdentity::from_resource(&resource);

    let stored = runtime
        .block_on(store_backup_artifact_from_async_reader(
            &identity,
            &mut reader,
            43_000,
            &root,
        ))
        .expect("async streamed backup");

    assert_eq!(
        std::fs::read(stored.artifact_file()).expect("artifact contents"),
        bytes
    );
    verify_stored_backup_artifact(&stored, 43_001).expect("verified async streamed backup");

    std::fs::remove_dir_all(&root).expect("remove async backup fixture");
}

#[cfg(unix)]
#[test]
fn logical_resources_in_one_shared_service_have_distinct_backup_identities() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-backup-logical-identity-{}-{}",
        std::process::id(),
        50_005
    ));
    let bill = logical_resource("bill/database", "bill");
    let shop = logical_resource("shop/database", "shop");
    let bill_identity = BackupResourceIdentity::from_logical(&bill, "install-1");
    let shop_identity = BackupResourceIdentity::from_logical(&shop, "install-1");

    let bill_backup =
        store_backup_artifact_for_identity(&bill_identity, b"bill data", 44_000, &root)
            .expect("bill backup");
    let shop_backup =
        store_backup_artifact_for_identity(&shop_identity, b"shop data", 44_000, &root)
            .expect("shop backup");

    assert_ne!(bill_backup, shop_backup);
    assert_eq!(
        std::fs::read(bill_backup.artifact_file()).expect("bill artifact"),
        b"bill data"
    );
    assert_eq!(
        std::fs::read(shop_backup.artifact_file()).expect("shop artifact"),
        b"shop data"
    );

    std::fs::remove_dir_all(&root).expect("remove logical backup fixture");
}

#[cfg(unix)]
#[test]
fn deletion_reverification_rejects_a_tampered_recovery_artifact() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-deletion-reverification-{}-{}",
        std::process::id(),
        50_006
    ));
    let logical = logical_resource("bill/database", "bill");
    let identity = BackupResourceIdentity::from_logical(&logical, "install-1");
    let stored = store_backup_artifact_for_identity(&identity, b"bill data", 44_000, &root)
        .expect("stored logical backup");
    let recovery = RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "backup-42".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        logical_resource_id: "bill/database".to_owned(),
        resource_kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        reference: stored.recovery_point().display().to_string(),
        artifact_sha256: "a33d2089e4c2c1afd8d0434f120070fc4b60b7a3429635739f82527b7f133dec"
            .to_owned(),
        artifact_size_bytes: 9,
        created_at_unix_seconds: 44_000,
        verified_at_unix_seconds: 44_001,
    })
    .expect("recovery point");
    verify_recovery_point_artifact(&recovery, &logical, "install-1", 44_002)
        .expect("unchanged recovery artifact");

    std::fs::write(stored.artifact_file(), b"tampered!").expect("tamper artifact");
    let error = verify_recovery_point_artifact(&recovery, &logical, "install-1", 44_003)
        .expect_err("tampered recovery artifact must fail closed");

    assert_eq!(error, BackupVerificationError::ChecksumMismatch);
    std::fs::remove_dir_all(&root).expect("remove deletion verification fixture");
}

#[cfg(unix)]
#[test]
fn stored_backup_open_rejects_linked_recovery_points() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "stackctl-backup-linked-reference-{}-{}",
        std::process::id(),
        50_006
    ));
    let resource = resource(
        ResourceRetention::Persistent,
        ResourceLifecycle::Orphaned,
        Some(1_000),
    );
    let stored = store_backup_artifact(&resource, b"recoverable bytes", 45_000, &root)
        .expect("stored linked fixture");
    let linked = root.join("linked-recovery-point");
    symlink(stored.recovery_point(), &linked).expect("link recovery point");

    let error = open_stored_backup_artifact(linked.to_str().expect("linked reference"))
        .expect_err("linked recovery point");

    assert_eq!(
        error.to_string(),
        format!(
            "backup recovery point '{}' must be a real directory",
            linked.display()
        )
    );

    std::fs::remove_dir_all(&root).expect("remove linked backup fixture");
}

#[cfg(unix)]
#[test]
fn restore_stages_verifies_and_commits_exact_verified_bytes() {
    let fixture = RestoreFixture::new("successful");
    let mut target = RecordingRestoreTarget::default();

    let evidence = restore_verified_backup(
        "restore-1",
        &fixture.resource,
        &fixture.stored,
        41_000,
        &mut target,
    )
    .expect("restored backup");

    assert_eq!(target.operations, ["stage", "verify", "commit"]);
    assert_eq!(target.staged_bytes, b"recoverable bytes");
    assert_eq!(evidence.artifact_size_bytes(), 17);
    assert!(!evidence.artifact_sha256().is_empty());

    fixture.remove();
}

#[cfg(unix)]
#[test]
fn restore_rejects_corruption_before_mutating_the_target() {
    let fixture = RestoreFixture::new("corrupt");
    std::fs::write(fixture.stored.artifact_file(), b"tampered").expect("tamper backup");
    let mut target = RecordingRestoreTarget::default();

    let error = restore_verified_backup(
        "restore-1",
        &fixture.resource,
        &fixture.stored,
        41_000,
        &mut target,
    )
    .expect_err("corrupt backup");

    assert_eq!(
        error.to_string(),
        "backup artifact checksum does not match its manifest"
    );
    assert!(target.operations.is_empty());

    fixture.remove();
}

#[cfg(unix)]
#[test]
fn restore_rejects_wrong_resource_evidence_before_mutating_the_target() {
    let fixture = RestoreFixture::new("wrong-resource");
    let other = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "resource-2".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "volume".to_owned(),
        compatibility_fingerprint: "sha256:fingerprint".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(1_000),
    });
    let mut target = RecordingRestoreTarget::default();

    let error = restore_verified_backup("restore-1", &other, &fixture.stored, 41_000, &mut target)
        .expect_err("wrong resource backup");

    assert_eq!(
        error.to_string(),
        "backup evidence does not match the restore resource"
    );
    assert!(target.operations.is_empty());

    fixture.remove();
}

#[cfg(unix)]
#[test]
fn restore_rejects_unsafe_restore_ids_before_mutating_the_target() {
    let fixture = RestoreFixture::new("unsafe-id");
    let mut target = RecordingRestoreTarget::default();

    for restore_id in ["", "../restore", "restore/child", "restore\0child"] {
        let error = restore_verified_backup(
            restore_id,
            &fixture.resource,
            &fixture.stored,
            41_000,
            &mut target,
        )
        .expect_err("unsafe restore id");

        assert_eq!(error.to_string(), "restore id must be non-empty and valid");
    }
    assert!(target.operations.is_empty());

    fixture.remove();
}

#[cfg(unix)]
#[test]
fn restore_rolls_back_when_the_target_consumes_only_a_prefix() {
    let fixture = RestoreFixture::new("prefix");
    let mut target = RecordingRestoreTarget {
        read_limit: Some(4),
        ..RecordingRestoreTarget::default()
    };

    let error = restore_verified_backup(
        "restore-1",
        &fixture.resource,
        &fixture.stored,
        41_000,
        &mut target,
    )
    .expect_err("partial restore");

    assert_eq!(
        error.to_string(),
        "restore target did not consume the complete backup artifact"
    );
    assert_eq!(target.operations, ["stage", "rollback"]);

    fixture.remove();
}

#[cfg(unix)]
#[test]
fn restore_rolls_back_target_verification_and_commit_failures() {
    for (failure, expected_operations) in [
        (RestoreFailure::Stage, vec!["stage", "rollback"]),
        (RestoreFailure::Verify, vec!["stage", "verify", "rollback"]),
        (
            RestoreFailure::Commit,
            vec!["stage", "verify", "commit", "rollback"],
        ),
    ] {
        let fixture = RestoreFixture::new(failure.label());
        let mut target = RecordingRestoreTarget {
            failure,
            ..RecordingRestoreTarget::default()
        };

        let error = restore_verified_backup(
            "restore-1",
            &fixture.resource,
            &fixture.stored,
            41_000,
            &mut target,
        )
        .expect_err("target failure");

        assert_eq!(
            error.to_string(),
            format!("restore target {failure} failed")
        );
        assert_eq!(target.operations, expected_operations);

        fixture.remove();
    }
}

#[cfg(unix)]
#[test]
fn restore_reports_both_primary_and_rollback_failures() {
    let fixture = RestoreFixture::new("rollback-failure");
    let mut target = RecordingRestoreTarget {
        failure: RestoreFailure::VerifyAndRollback,
        ..RecordingRestoreTarget::default()
    };

    let error = restore_verified_backup(
        "restore-1",
        &fixture.resource,
        &fixture.stored,
        41_000,
        &mut target,
    )
    .expect_err("rollback failure");

    assert_eq!(
        error.to_string(),
        "restore target verify failed; rollback also failed: restore target rollback failed"
    );
    assert_eq!(target.operations, ["stage", "verify", "rollback"]);

    fixture.remove();
}

#[cfg(unix)]
struct RestoreFixture {
    root: std::path::PathBuf,
    resource: ResourceRecord,
    stored: super::StoredBackupArtifact,
}

#[cfg(unix)]
impl RestoreFixture {
    fn new(label: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("stackctl-restore-{label}-{}", std::process::id()));
        let resource = resource(
            ResourceRetention::Persistent,
            ResourceLifecycle::Orphaned,
            Some(1_000),
        );
        let stored = store_backup_artifact(&resource, b"recoverable bytes", 40_000, &root)
            .expect("stored restore fixture");

        Self {
            root,
            resource,
            stored,
        }
    }

    fn remove(self) {
        std::fs::remove_dir_all(self.root).expect("remove restore fixture");
    }
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum RestoreFailure {
    #[default]
    None,
    Stage,
    Verify,
    Commit,
    VerifyAndRollback,
}

#[cfg(unix)]
impl RestoreFailure {
    const fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Stage => "stage",
            Self::Verify => "verify",
            Self::Commit => "commit",
            Self::VerifyAndRollback => "verify-rollback",
        }
    }
}

#[cfg(unix)]
impl std::fmt::Display for RestoreFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

#[cfg(unix)]
#[derive(Default)]
struct RecordingRestoreTarget {
    operations: Vec<&'static str>,
    staged_bytes: Vec<u8>,
    read_limit: Option<u64>,
    failure: RestoreFailure,
}

#[cfg(unix)]
impl RestoreTarget for RecordingRestoreTarget {
    fn stage(
        &mut self,
        _restore_id: &str,
        _resource: &ResourceRecord,
        input: &mut dyn std::io::Read,
    ) -> Result<(), RestoreTargetError> {
        self.operations.push("stage");
        if self.failure == RestoreFailure::Stage {
            return Err(RestoreTargetError::new("restore target stage failed"));
        }
        if let Some(read_limit) = self.read_limit {
            let mut prefix = vec![0_u8; usize::try_from(read_limit).expect("read limit")];
            input.read_exact(&mut prefix).expect("read staged prefix");
            self.staged_bytes.extend(prefix);
        } else {
            input
                .read_to_end(&mut self.staged_bytes)
                .expect("read staged backup");
        }

        Ok(())
    }

    fn verify(
        &mut self,
        _restore_id: &str,
        _resource: &ResourceRecord,
    ) -> Result<(), RestoreTargetError> {
        self.operations.push("verify");
        if matches!(
            self.failure,
            RestoreFailure::Verify | RestoreFailure::VerifyAndRollback
        ) {
            return Err(RestoreTargetError::new("restore target verify failed"));
        }

        Ok(())
    }

    fn commit(
        &mut self,
        _restore_id: &str,
        _resource: &ResourceRecord,
    ) -> Result<(), RestoreTargetError> {
        self.operations.push("commit");
        if self.failure == RestoreFailure::Commit {
            return Err(RestoreTargetError::new("restore target commit failed"));
        }

        Ok(())
    }

    fn rollback(
        &mut self,
        _restore_id: &str,
        _resource: &ResourceRecord,
    ) -> Result<(), RestoreTargetError> {
        self.operations.push("rollback");
        if self.failure == RestoreFailure::VerifyAndRollback {
            return Err(RestoreTargetError::new("restore target rollback failed"));
        }

        Ok(())
    }
}

#[cfg(unix)]
struct ChunkedReader<'bytes> {
    bytes: &'bytes [u8],
    offset: usize,
    maximum_chunk: usize,
}

#[cfg(unix)]
impl<'bytes> ChunkedReader<'bytes> {
    const fn new(bytes: &'bytes [u8], maximum_chunk: usize) -> Self {
        Self {
            bytes,
            offset: 0,
            maximum_chunk,
        }
    }
}

#[cfg(unix)]
impl std::io::Read for ChunkedReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let remaining = &self.bytes[self.offset..];
        let count = remaining.len().min(buffer.len()).min(self.maximum_chunk);
        buffer[..count].copy_from_slice(&remaining[..count]);
        self.offset += count;

        Ok(count)
    }
}

fn resource(
    retention: ResourceRetention,
    lifecycle: ResourceLifecycle,
    orphaned_at_unix_seconds: Option<i64>,
) -> ResourceRecord {
    ResourceRecord::new(ResourceRecordOptions {
        resource_id: "resource-1".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "volume".to_owned(),
        compatibility_fingerprint: "sha256:fingerprint".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention,
        lifecycle,
        orphaned_at_unix_seconds,
    })
}

fn logical_resource(logical_resource_id: &str, project_id: &str) -> LogicalResourceRecord {
    LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: logical_resource_id.to_owned(),
        shared_resource_id: "postgres-shared-17".to_owned(),
        project_id: project_id.to_owned(),
        service_id: "database".to_owned(),
        kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
}

fn logical_resource_of_kind(kind: &str) -> LogicalResourceRecord {
    LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/database/data".to_owned(),
        shared_resource_id: "shared-service".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: kind.to_owned(),
        compatibility_fingerprint: "sha256:compatibility".to_owned(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
}

fn prune_logical(
    lifecycle: ResourceLifecycle,
    orphaned_at_unix_seconds: Option<i64>,
) -> LogicalResourceRecord {
    LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "stackctl_bill_database".to_owned(),
        shared_resource_id: "postgres-shared-17".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        desired_revision: "sha256:desired-v1".to_owned(),
        lifecycle,
        orphaned_at_unix_seconds,
    })
}

fn prune_credential(lifecycle: CredentialLifecycle) -> CredentialRecord {
    CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/postgresql".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "stackctl_bill_database_role".to_owned(),
        secret: "redacted-test-secret".to_owned(),
        lifecycle,
    })
}

fn prune_recovery_point(id: &str, checksum_character: &str) -> RecoveryPointRecord {
    prune_recovery_point_at(id, checksum_character, 9_000)
}

fn prune_recovery_point_at(
    id: &str,
    checksum_character: &str,
    created_at_unix_seconds: i64,
) -> RecoveryPointRecord {
    RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: id.to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        logical_resource_id: "stackctl_bill_database".to_owned(),
        resource_kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        reference: format!("/backups/{id}"),
        artifact_sha256: checksum_character.repeat(64),
        artifact_size_bytes: 1_024,
        created_at_unix_seconds,
        verified_at_unix_seconds: created_at_unix_seconds + 1,
    })
    .expect("valid recovery point")
}

fn mysql_prune_logical() -> LogicalResourceRecord {
    LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "stackctl_bill_database".to_owned(),
        shared_resource_id: "mysql-shared-8".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "mysql_database".to_owned(),
        compatibility_fingerprint: "sha256:mysql-8".to_owned(),
        desired_revision: "sha256:desired-v1".to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(10_000),
    })
}

fn mysql_prune_credential() -> CredentialRecord {
    CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/mysql".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "st_bill_database".to_owned(),
        secret: "redacted-test-secret".to_owned(),
        lifecycle: CredentialLifecycle::Disabled,
    })
}

fn mysql_prune_recovery_point() -> RecoveryPointRecord {
    RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "backup-mysql".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        logical_resource_id: "stackctl_bill_database".to_owned(),
        resource_kind: "mysql_database".to_owned(),
        compatibility_fingerprint: "sha256:mysql-8".to_owned(),
        reference: "/backups/backup-mysql".to_owned(),
        artifact_sha256: "b".repeat(64),
        artifact_size_bytes: 2_048,
        created_at_unix_seconds: 9_000,
        verified_at_unix_seconds: 9_001,
    })
    .expect("valid MySQL recovery point")
}
