use super::{
    CompatibilityFingerprint, CompatibilityFingerprintOptions, CompatibilityProfile,
    CredentialEntropy, CredentialGenerationError, CredentialSecret, IsolationCapability,
    MySqlFlavor, MySqlSharedInstancePlan, MySqlSharedInstancePlanOptions, PersistenceMode,
    PostgresLogicalResourcePlan, PostgresSharedInstancePlan, PostgresSharedInstancePlanOptions,
    RedisAclProject, RedisAclSnapshot, RedisFlavor, RedisSharedInstancePlan,
    RedisSharedInstancePlanOptions, SharedServiceRequest, generate_credential_secret,
    plan_mysql_project_resources, plan_postgres_project_resources, plan_redis_project_resources,
    plan_shared_instances, provision_mysql_logical_resource, provision_postgres_logical_resource,
    store_redis_acl_snapshot,
};
use crate::control_plane::engine::{
    CommandExecutionId, CommandExecutor, CommandRequest, CommandSession, CommandStatus,
    ContainerId, ContainerLogStream, EngineFuture, ObservedContainer, OwnedContainer,
    reconstruct_owned_container,
};
use futures_util::stream;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use tokio::io::AsyncReadExt;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[test]
fn equivalent_postgres_profiles_share_one_fingerprint() {
    let first = fingerprint(vec!["postgis", "pg_stat_statements"], "17");
    let second = fingerprint(vec!["pg_stat_statements", "postgis"], "17");

    assert_eq!(first, second);
    assert_eq!(first.as_str().len(), 71);
    assert!(first.as_str().starts_with("sha256:"));
}

#[test]
fn different_major_versions_never_share_a_fingerprint() {
    let postgres_17 = fingerprint(vec!["postgis"], "17");
    let postgres_18 = fingerprint(vec!["postgis"], "18");

    assert_ne!(postgres_17, postgres_18);
}

#[test]
fn immutable_settings_participate_in_the_fingerprint() {
    let mut options = postgres_options(vec!["postgis"], "17");
    let baseline =
        CompatibilityFingerprint::from_options(options.clone()).expect("baseline fingerprint");
    options
        .immutable_settings
        .insert("locale".to_owned(), "fi_FI.UTF-8".to_owned());

    let localized = CompatibilityFingerprint::from_options(options).expect("localized fingerprint");

    assert_ne!(baseline, localized);
}

#[test]
fn incomplete_compatibility_profiles_fail_before_planning() {
    let mut options = postgres_options(Vec::new(), "17");
    options.implementation.clear();

    let error = CompatibilityFingerprint::from_options(options)
        .expect_err("missing implementation identity");

    assert_eq!(
        error.to_string(),
        "compatibility implementation must not be empty"
    );
}

#[test]
fn mutable_image_references_cannot_identify_shared_instances() {
    let mut options = postgres_options(Vec::new(), "17");
    options.image_digest = "postgres:17".to_owned();

    let error =
        CompatibilityFingerprint::from_options(options).expect_err("mutable image reference");

    assert_eq!(
        error.to_string(),
        "compatibility image 'postgres:17' must use an immutable sha256 digest"
    );
}

#[test]
fn forty_projects_across_two_postgres_majors_plan_two_instances() {
    let requests = (0..40)
        .map(|index| {
            let major = if index < 20 { "17" } else { "18" };

            SharedServiceRequest::new(
                format!("project-{index:02}"),
                "database",
                profile(vec!["postgis"], major),
            )
        })
        .collect();

    let plans = plan_shared_instances(requests);

    assert_eq!(plans.len(), 2);
    assert_eq!(
        plans
            .iter()
            .map(|plan| plan.consumers().len())
            .collect::<Vec<_>>(),
        vec![20, 20]
    );
    assert_eq!(
        plans
            .iter()
            .map(|plan| plan.fingerprint())
            .collect::<BTreeSet<_>>()
            .len(),
        2
    );
    assert_eq!(plans[0].profile().implementation(), "postgresql");
    assert!(plans[0].profile().image_digest().contains("@sha256:"));
}

#[test]
fn repeated_identical_consumers_do_not_duplicate_logical_ownership() {
    let request = SharedServiceRequest::new("bill", "database", profile(Vec::new(), "17"));

    let plans = plan_shared_instances(vec![request.clone(), request]);
    let consumers = plans
        .first()
        .map(|plan| plan.consumers())
        .unwrap_or_default();

    assert_eq!(plans.len(), 1);
    assert_eq!(consumers.len(), 1);
    assert_eq!(
        consumers.first().map(|owner| owner.project_id()),
        Some("bill")
    );
    assert_eq!(
        consumers.first().map(|owner| owner.service_id()),
        Some("database")
    );
}

#[test]
fn managed_credentials_use_256_bits_of_injected_entropy_and_redact_debug() {
    let secret = generate_credential_secret(&SequentialEntropy).expect("managed secret");

    assert_eq!(
        secret.expose(),
        "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
    );
    assert_eq!(format!("{secret:?}"), "CredentialSecret([REDACTED])");
}

#[test]
fn redis_acl_snapshots_are_complete_deterministic_and_redacted() {
    let snapshot = RedisAclSnapshot::new(
        CredentialSecret::new("admin-secret".to_owned()),
        vec![
            RedisAclProject::new(
                "shop",
                "cache",
                CredentialSecret::new("shop-secret".to_owned()),
            )
            .expect("shop ACL"),
            RedisAclProject::new(
                "bill",
                "cache",
                CredentialSecret::new("bill-secret".to_owned()),
            )
            .expect("bill ACL"),
        ],
    )
    .expect("ACL snapshot");

    assert_eq!(
        snapshot.contents(),
        "user default off resetpass resetkeys resetchannels -@all\n\
         user stackctl_admin on resetpass #16175223c8ddce5ace0493c948569c211b03c4c6bb3d3e484434999448cffe01 resetkeys ~* resetchannels &* +@all\n\
         user st_bill_cache on resetpass #fdb34f0710b2f482f4eb9dded04a6777f64c7388e42b0553ad43b26e126b029c resetkeys ~stackctl:bill:cache:* resetchannels &stackctl:bill:cache:* -@all +@read +@write +@connection +@transaction +@pubsub +@scripting\n\
         user st_shop_cache on resetpass #3c655a3878fd8e4145a5facca30188ce74792ddff5d57aaf2203bbe74a940cb5 resetkeys ~stackctl:shop:cache:* resetchannels &stackctl:shop:cache:* -@all +@read +@write +@connection +@transaction +@pubsub +@scripting\n"
    );
    assert!(!snapshot.contents().contains("secret"));
    assert_eq!(
        format!("{snapshot:?}"),
        "RedisAclSnapshot { user_count: 3 }"
    );
    assert!(!format!("{snapshot:?}").contains("secret"));
}

#[test]
fn redis_acl_snapshots_reject_duplicate_project_users() {
    let projects = vec![
        RedisAclProject::new(
            "bill",
            "cache",
            CredentialSecret::new("first-secret".to_owned()),
        )
        .expect("first ACL"),
        RedisAclProject::new(
            "bill",
            "cache",
            CredentialSecret::new("second-secret".to_owned()),
        )
        .expect("second ACL"),
    ];

    let error = RedisAclSnapshot::new(CredentialSecret::new("admin".to_owned()), projects)
        .expect_err("duplicate ACL user");

    assert_eq!(
        error.to_string(),
        "Redis ACL user 'st_bill_cache' is defined more than once"
    );
}

#[cfg(unix)]
#[test]
fn redis_acl_store_atomically_replaces_a_private_directory_mounted_file() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-redis-acl-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    drop(std::fs::remove_dir_all(&root));
    let initial = RedisAclSnapshot::new(CredentialSecret::new("admin-one".to_owned()), Vec::new())
        .expect("initial ACL");
    let replacement =
        RedisAclSnapshot::new(CredentialSecret::new("admin-two".to_owned()), Vec::new())
            .expect("replacement ACL");

    let stored = store_redis_acl_snapshot(&initial, &root).expect("store initial ACL");
    store_redis_acl_snapshot(&replacement, &root).expect("replace ACL");

    assert_eq!(stored.directory(), root);
    assert_eq!(stored.mount_directory(), root.join("mounted"));
    assert_eq!(stored.acl_file(), root.join("mounted/users.acl"));
    assert_eq!(
        std::fs::read_to_string(stored.acl_file()).expect("stored ACL"),
        replacement.contents()
    );
    assert_eq!(
        std::fs::metadata(stored.directory())
            .expect("ACL directory")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(stored.mount_directory())
            .expect("ACL mount directory")
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    assert_eq!(
        std::fs::metadata(stored.acl_file())
            .expect("ACL file")
            .permissions()
            .mode()
            & 0o777,
        0o644
    );
    assert_eq!(
        std::fs::read_dir(stored.mount_directory())
            .expect("ACL mount directory entries")
            .count(),
        1
    );

    std::fs::remove_dir_all(&root).expect("remove ACL fixture");
}

#[test]
fn redis_and_valkey_materialize_as_separate_private_acl_backed_instances() {
    let redis_shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "cache",
        cache_profile("redis", "8", PersistenceMode::Persistent),
    )])
    .pop()
    .expect("shared Redis plan");
    let valkey_shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "shop",
        "cache",
        cache_profile("valkey", "9", PersistenceMode::Ephemeral),
    )])
    .pop()
    .expect("shared Valkey plan");
    let redis = RedisSharedInstancePlan::new(
        &redis_shared,
        RedisSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:redis-v1".to_owned(),
            acl_directory: "/private/redis-acl".into(),
            bootstrap_secret: CredentialSecret::new("redis-admin".to_owned()),
        },
    )
    .expect("Redis instance");
    let valkey = RedisSharedInstancePlan::new(
        &valkey_shared,
        RedisSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:valkey-v1".to_owned(),
            acl_directory: "/private/valkey-acl".into(),
            bootstrap_secret: CredentialSecret::new("valkey-admin".to_owned()),
        },
    )
    .expect("Valkey instance");

    assert_eq!(redis.flavor(), RedisFlavor::Redis);
    assert_eq!(valkey.flavor(), RedisFlavor::Valkey);
    assert_ne!(redis.container().name(), valkey.container().name());
    assert_eq!(redis.acl_mount_target(), "/etc/stackctl/acl");
    assert_eq!(redis.acl_file(), "/etc/stackctl/acl/users.acl");
    assert_eq!(redis.data_mount_target(), "/data");
    assert!(redis.volume().is_some());
    assert!(valkey.volume().is_none());
    assert_eq!(
        redis.command_arguments(),
        [
            "redis-server",
            "--aclfile",
            "/etc/stackctl/acl/users.acl",
            "--appendonly",
            "yes",
        ]
    );
    assert_eq!(
        valkey.command_arguments(),
        [
            "valkey-server",
            "--aclfile",
            "/etc/stackctl/acl/users.acl",
            "--appendonly",
            "no",
        ]
    );
    assert!(format!("{:?}", redis.container()).contains("/private/redis-acl"));
    assert!(format!("{:?}", redis.container()).contains("read_only: true"));
    assert!(!format!("{:?}", redis.container()).contains("redis-admin"));
    assert_eq!(redis.bootstrap_credential().username(), "stackctl_admin");
    assert_eq!(redis.bootstrap_credential().project_id(), None);
}

#[test]
fn redis_project_resources_compose_acl_credential_and_managed_environment() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "cache",
        cache_profile("redis", "8", PersistenceMode::Persistent),
    )])
    .pop()
    .expect("shared Redis plan");
    let instance = RedisSharedInstancePlan::new(
        &shared,
        RedisSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:redis-v1".to_owned(),
            acl_directory: "/private/redis-acl".into(),
            bootstrap_secret: CredentialSecret::new("redis-admin".to_owned()),
        },
    )
    .expect("Redis instance");

    let project = plan_redis_project_resources(
        "bill",
        "cache",
        &instance,
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("Redis project resources");

    assert_eq!(project.acl().username(), "st_bill_cache");
    assert_eq!(project.acl().prefix(), "stackctl:bill:cache:");
    assert_eq!(project.credential().credential_id(), "bill/cache/redis");
    assert_eq!(project.credential().username(), "st_bill_cache");
    assert_eq!(project.credential().secret(), "project-secret");
    assert_eq!(
        project.environment().values(),
        &BTreeMap::from([
            (
                "REDIS_HOST".to_owned(),
                instance.container().name().to_owned()
            ),
            ("REDIS_PASSWORD".to_owned(), "project-secret".to_owned()),
            ("REDIS_PORT".to_owned(), "6379".to_owned()),
            ("REDIS_PREFIX".to_owned(), "stackctl:bill:cache:".to_owned()),
            ("REDIS_USERNAME".to_owned(), "st_bill_cache".to_owned()),
        ])
    );
    assert!(!format!("{project:?}").contains("project-secret"));
}

#[test]
fn postgres_logical_resources_use_deterministic_isolated_names_and_stdin() {
    let plan = PostgresLogicalResourcePlan::new(
        "bill",
        "database",
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("PostgreSQL logical plan");

    assert_eq!(plan.database_name(), "stackctl_bill_database");
    assert_eq!(plan.role_name(), "stackctl_bill_database_role");
    assert_eq!(plan.credential_id(), "bill/database/postgresql");
    assert_eq!(
        plan.command_arguments(),
        [
            "psql",
            "--no-psqlrc",
            "--set=ON_ERROR_STOP=1",
            "--username=postgres",
            "--dbname=postgres",
        ]
    );
    assert!(plan.stdin_sql().contains("CREATE DATABASE"));
    assert!(plan.stdin_sql().contains("REVOKE ALL"));
    assert!(plan.stdin_sql().contains("project-secret"));
    assert!(!format!("{plan:?}").contains("project-secret"));
}

#[test]
fn postgres_logical_resources_fail_instead_of_shortening_identifiers() {
    let project_id = "a".repeat(50);

    let error = PostgresLogicalResourcePlan::new(
        &project_id,
        "database",
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect_err("overlong PostgreSQL role");

    assert!(
        error
            .to_string()
            .contains("exceeds PostgreSQL's 63-byte limit")
    );
}

#[test]
fn postgres_shared_instances_materialize_one_private_persistent_container() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        profile(Vec::new(), "17"),
    )])
    .pop()
    .expect("shared PostgreSQL plan");
    let plan = PostgresSharedInstancePlan::new(
        &shared,
        PostgresSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:desired-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("root-secret".to_owned()),
        },
    )
    .expect("PostgreSQL instance plan");

    assert!(plan.container().name().starts_with("stackctl-shared-"));
    assert_eq!(plan.container().image(), shared.profile().image_digest());
    assert_eq!(
        plan.container()
            .metadata()
            .labels()
            .get("dev.stackctl.kind"),
        Some(&"shared_service".to_owned())
    );
    assert_eq!(plan.data_mount_target(), "/var/lib/postgresql/data");
    assert!(plan.volume().is_some());
    assert_eq!(plan.bootstrap_credential().project_id(), None);
    assert_eq!(plan.bootstrap_credential().secret(), "root-secret");
    assert!(!format!("{:?}", plan.bootstrap_credential()).contains("root-secret"));
    assert!(!format!("{:?}", plan.container()).contains("root-secret"));
}

#[test]
fn postgres_eighteen_uses_the_new_parent_volume_mount() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        profile(Vec::new(), "18"),
    )])
    .pop()
    .expect("shared PostgreSQL plan");
    let plan = PostgresSharedInstancePlan::new(
        &shared,
        PostgresSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:desired-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("root-secret".to_owned()),
        },
    )
    .expect("PostgreSQL 18 instance plan");

    assert_eq!(plan.data_mount_target(), "/var/lib/postgresql");
}

#[test]
fn postgres_logical_provisioning_streams_secret_sql_and_checks_exit_status() {
    let plan = PostgresLogicalResourcePlan::new(
        "bill",
        "database",
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("PostgreSQL logical plan");
    let metadata = crate::control_plane::engine::ManagedResourceMetadata::new(
        crate::control_plane::engine::ManagedResourceMetadataOptions {
            installation_id: "install-1".to_owned(),
            kind: crate::control_plane::engine::ResourceKind::SharedService,
            project_id: None,
            compatibility_fingerprint: "sha256:postgres-17".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:desired-v1".to_owned(),
            retention: crate::control_plane::engine::RetentionClass::Persistent,
        },
    )
    .expect("owned metadata");
    let observed =
        ObservedContainer::new(ContainerId::new("postgres-container"), metadata.labels());
    let container =
        reconstruct_owned_container(&observed, "install-1", 8).expect("owned container handle");
    let executor = RecordingPostgresExecutor::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    runtime
        .block_on(provision_postgres_logical_resource(
            &executor, &container, &plan,
        ))
        .expect("provision PostgreSQL resource");
    runtime.block_on(tokio::task::yield_now());

    let stdin = executor.stdin.lock().expect("recorded stdin").clone();
    let request_debug = executor
        .request_debug
        .lock()
        .expect("recorded request")
        .clone();
    assert!(
        String::from_utf8(stdin)
            .expect("SQL UTF-8")
            .contains("project-secret")
    );
    assert!(!request_debug.contains("project-secret"));
    assert!(request_debug.contains("argument_count: 5"));
}

#[test]
fn postgres_project_resources_emit_stable_credentials_and_managed_environment() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        profile(Vec::new(), "17"),
    )])
    .pop()
    .expect("shared PostgreSQL plan");
    let instance = PostgresSharedInstancePlan::new(
        &shared,
        PostgresSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:desired-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("root-secret".to_owned()),
        },
    )
    .expect("PostgreSQL instance plan");

    let project = plan_postgres_project_resources(
        "bill",
        "database",
        &instance,
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("PostgreSQL project resources");

    assert_eq!(
        project.credential().username(),
        "stackctl_bill_database_role"
    );
    assert_eq!(project.credential().secret(), "project-secret");
    assert_eq!(
        project.environment().values(),
        &BTreeMap::from([
            ("DB_CONNECTION".to_owned(), "pgsql".to_owned()),
            (
                "DB_DATABASE".to_owned(),
                "stackctl_bill_database".to_owned()
            ),
            ("DB_HOST".to_owned(), instance.container().name().to_owned()),
            ("DB_PASSWORD".to_owned(), "project-secret".to_owned()),
            ("DB_PORT".to_owned(), "5432".to_owned()),
            (
                "DB_USERNAME".to_owned(),
                "stackctl_bill_database_role".to_owned()
            ),
        ])
    );
    assert!(!format!("{project:?}").contains("project-secret"));
}

#[test]
fn mysql_and_mariadb_materialize_as_separate_private_instances() {
    let mysql_shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        sql_profile("mysql", "8"),
    )])
    .pop()
    .expect("shared MySQL plan");
    let maria_shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "shop",
        "database",
        sql_profile("mariadb", "11"),
    )])
    .pop()
    .expect("shared MariaDB plan");
    let mysql = MySqlSharedInstancePlan::new(
        &mysql_shared,
        MySqlSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:mysql-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("mysql-root".to_owned()),
        },
    )
    .expect("MySQL instance");
    let maria = MySqlSharedInstancePlan::new(
        &maria_shared,
        MySqlSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:mariadb-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("maria-root".to_owned()),
        },
    )
    .expect("MariaDB instance");

    assert_eq!(mysql.flavor(), MySqlFlavor::MySql);
    assert_eq!(maria.flavor(), MySqlFlavor::MariaDb);
    assert_ne!(mysql.container().name(), maria.container().name());
    assert_eq!(mysql.data_mount_target(), "/var/lib/mysql");
    assert!(mysql.volume().is_some());
    assert!(maria.volume().is_some());
    assert!(!format!("{:?}", mysql.bootstrap_credential()).contains("mysql-root"));
}

#[test]
fn mysql_project_resources_isolate_schema_user_and_application_environment() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        sql_profile("mysql", "8"),
    )])
    .pop()
    .expect("shared MySQL plan");
    let instance = MySqlSharedInstancePlan::new(
        &shared,
        MySqlSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:mysql-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("mysql-root".to_owned()),
        },
    )
    .expect("MySQL instance");
    let project = plan_mysql_project_resources(
        "bill",
        "database",
        &instance,
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("MySQL project resources");

    assert_eq!(project.logical().schema_name(), "stackctl_bill_database");
    assert_eq!(project.logical().username(), "st_bill_database");
    assert!(
        project
            .logical()
            .stdin_sql()
            .contains("GRANT ALL PRIVILEGES")
    );
    assert_eq!(
        project.environment().values().get("DB_CONNECTION"),
        Some(&"mysql".to_owned())
    );
    assert_eq!(
        project.environment().values().get("DB_HOST"),
        Some(&instance.container().name().to_owned())
    );
    assert!(!format!("{project:?}").contains("project-secret"));
}

#[test]
fn mysql_logical_provisioning_keeps_both_passwords_out_of_command_debug() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        sql_profile("mysql", "8"),
    )])
    .pop()
    .expect("shared MySQL plan");
    let instance = MySqlSharedInstancePlan::new(
        &shared,
        MySqlSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:mysql-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("mysql-root".to_owned()),
        },
    )
    .expect("MySQL instance");
    let project = plan_mysql_project_resources(
        "bill",
        "database",
        &instance,
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("MySQL project resources");
    let container = owned_shared_container("mysql-container", "sha256:mysql-8");
    let executor = RecordingPostgresExecutor::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    runtime
        .block_on(provision_mysql_logical_resource(
            &executor,
            &container,
            &instance,
            project.logical(),
        ))
        .expect("provision MySQL resource");
    runtime.block_on(tokio::task::yield_now());

    let stdin = executor.stdin.lock().expect("recorded stdin").clone();
    let request_debug = executor
        .request_debug
        .lock()
        .expect("recorded request")
        .clone();
    assert!(
        String::from_utf8(stdin)
            .expect("SQL UTF-8")
            .contains("project-secret")
    );
    assert!(request_debug.contains("MYSQL_PWD"));
    assert!(!request_debug.contains("mysql-root"));
    assert!(!request_debug.contains("project-secret"));
}

struct SequentialEntropy;

impl CredentialEntropy for SequentialEntropy {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), CredentialGenerationError> {
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::try_from(index).expect("test entropy index");
        }

        Ok(())
    }
}

#[derive(Default)]
struct RecordingPostgresExecutor {
    stdin: Arc<Mutex<Vec<u8>>>,
    request_debug: Arc<Mutex<String>>,
}

impl CommandExecutor for RecordingPostgresExecutor {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        let stdin = Arc::clone(&self.stdin);
        *self.request_debug.lock().expect("request debug lock") = format!("{request:?}");
        let container_id = container.id().clone();

        Box::pin(async move {
            let (writer, mut reader) = tokio::io::duplex(16 * 1024);
            tokio::spawn(async move {
                let mut bytes = Vec::new();
                reader
                    .read_to_end(&mut bytes)
                    .await
                    .expect("read provisioning stdin");
                *stdin.lock().expect("stdin lock") = bytes;
            });
            let output: ContainerLogStream<'static> = Box::pin(stream::empty());

            Ok(CommandSession::new(
                CommandExecutionId::new("exec-1"),
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
        Box::pin(async { Ok(CommandStatus::Exited(0)) })
    }
}

fn fingerprint(extensions: Vec<&str>, major_version: &str) -> CompatibilityFingerprint {
    CompatibilityFingerprint::from_options(postgres_options(extensions, major_version))
        .expect("valid compatibility fingerprint")
}

fn profile(extensions: Vec<&str>, major_version: &str) -> CompatibilityProfile {
    CompatibilityProfile::from_options(postgres_options(extensions, major_version))
        .expect("valid compatibility profile")
}

fn sql_profile(implementation: &str, major_version: &str) -> CompatibilityProfile {
    CompatibilityProfile::from_options(CompatibilityFingerprintOptions {
        implementation: implementation.to_owned(),
        major_version: major_version.to_owned(),
        image_digest: format!("{implementation}@sha256:{}", "b".repeat(64)),
        extensions: Vec::new(),
        immutable_settings: BTreeMap::new(),
        persistence: PersistenceMode::Persistent,
        isolation: IsolationCapability::DatabaseAndRole,
        platform_architecture: Some("linux/arm64".to_owned()),
    })
    .expect("valid SQL compatibility profile")
}

fn cache_profile(
    implementation: &str,
    major_version: &str,
    persistence: PersistenceMode,
) -> CompatibilityProfile {
    CompatibilityProfile::from_options(CompatibilityFingerprintOptions {
        implementation: implementation.to_owned(),
        major_version: major_version.to_owned(),
        image_digest: format!("{implementation}@sha256:{}", "c".repeat(64)),
        extensions: Vec::new(),
        immutable_settings: BTreeMap::new(),
        persistence,
        isolation: IsolationCapability::AclAndPrefix,
        platform_architecture: Some("linux/arm64".to_owned()),
    })
    .expect("valid cache compatibility profile")
}

fn owned_shared_container(id: &str, fingerprint: &str) -> OwnedContainer {
    let metadata = crate::control_plane::engine::ManagedResourceMetadata::new(
        crate::control_plane::engine::ManagedResourceMetadataOptions {
            installation_id: "install-1".to_owned(),
            kind: crate::control_plane::engine::ResourceKind::SharedService,
            project_id: None,
            compatibility_fingerprint: fingerprint.to_owned(),
            schema_version: 8,
            desired_revision: "sha256:desired-v1".to_owned(),
            retention: crate::control_plane::engine::RetentionClass::Persistent,
        },
    )
    .expect("owned metadata");
    let observed = ObservedContainer::new(ContainerId::new(id), metadata.labels());

    reconstruct_owned_container(&observed, "install-1", 8).expect("owned container handle")
}

fn postgres_options(extensions: Vec<&str>, major_version: &str) -> CompatibilityFingerprintOptions {
    CompatibilityFingerprintOptions {
        implementation: "postgresql".to_owned(),
        major_version: major_version.to_owned(),
        image_digest: concat!(
            "postgres@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )
        .to_owned(),
        extensions: extensions.into_iter().map(str::to_owned).collect(),
        immutable_settings: BTreeMap::from([(
            "authentication".to_owned(),
            "scram-sha-256".to_owned(),
        )]),
        persistence: PersistenceMode::Persistent,
        isolation: IsolationCapability::DatabaseAndRole,
        platform_architecture: Some("linux/arm64".to_owned()),
    }
}
