use super::{
    CompatibilityFingerprint, CompatibilityFingerprintOptions, CompatibilityProfile,
    CredentialEntropy, CredentialGenerationError, CredentialSecret, IsolationCapability,
    MongoDbLogicalResourcePlan, MongoDbSharedInstancePlan, MongoDbSharedInstancePlanOptions,
    MySqlFlavor, MySqlSharedInstancePlan, MySqlSharedInstancePlanOptions, PersistenceMode,
    PostgresLogicalResourcePlan, PostgresSharedInstancePlan, PostgresSharedInstancePlanOptions,
    RabbitMqDefinitions, RabbitMqPasswordHash, RabbitMqProjectDefinition,
    RabbitMqSharedInstancePlan, RabbitMqSharedInstancePlanOptions, RedisAclProject,
    RedisAclSnapshot, RedisFlavor, RedisSharedInstancePlan, RedisSharedInstancePlanOptions,
    SharedServiceRequest, generate_credential_secret, plan_mysql_project_resources,
    plan_postgres_project_resources, plan_rabbitmq_project_resources, plan_redis_project_resources,
    plan_shared_instances, provision_mysql_logical_resource, provision_postgres_logical_resource,
    reload_rabbitmq_definitions, reload_redis_acl, store_credential_secret,
    store_rabbitmq_definitions, store_redis_acl_snapshot,
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
fn mongodb_shared_instances_use_private_secret_files_and_retained_data() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "database",
        mongodb_profile("8"),
    )])
    .pop()
    .expect("shared MongoDB plan");
    let plan = MongoDbSharedInstancePlan::new(
        &shared,
        MongoDbSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:mongodb-v1".to_owned(),
            bootstrap_secret: CredentialSecret::new("mongo-root".to_owned()),
            bootstrap_secret_file: "/private/mongodb/root-password".into(),
        },
    )
    .expect("MongoDB instance");

    assert_eq!(plan.data_mount_target(), "/data/db");
    assert_eq!(
        plan.bootstrap_secret_target(),
        "/run/stackctl-secrets/mongodb-root-password"
    );
    assert!(plan.volume().is_some());
    assert_eq!(plan.bootstrap_credential().username(), "stackctl_admin");
    let debug = format!("{:?}", plan.container());
    assert!(debug.contains("/private/mongodb/root-password"));
    assert!(debug.contains("read_only: true"));
    assert!(debug.contains("MONGO_INITDB_ROOT_PASSWORD_FILE"));
    assert!(!debug.contains("mongo-root"));
}

#[cfg(unix)]
#[test]
fn managed_secret_store_is_private_immutable_and_exact() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-managed-secret-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    drop(std::fs::remove_dir_all(&root));
    let path = root.join("mongodb-root");
    let secret = CredentialSecret::new("root-secret".to_owned());

    let stored = store_credential_secret(&secret, &path).expect("store secret");
    store_credential_secret(&secret, &path).expect("reconcile secret");
    let error =
        store_credential_secret(&CredentialSecret::new("different-secret".to_owned()), &path)
            .expect_err("reject secret replacement");

    assert_eq!(stored, path);
    assert_eq!(
        std::fs::read_to_string(&path).expect("secret file"),
        "root-secret"
    );
    assert_eq!(
        std::fs::metadata(&root)
            .expect("secret directory")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(&path)
            .expect("secret file")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(error.to_string().contains("refusing to replace"));
    assert!(!error.to_string().contains("root-secret"));
    assert!(!error.to_string().contains("different-secret"));

    std::fs::remove_dir_all(&root).expect("remove managed-secret fixture");
}

#[test]
fn mongodb_logical_resources_are_idempotent_database_scoped_and_stdin_only() {
    let plan = MongoDbLogicalResourcePlan::new(
        "bill",
        "database",
        CredentialSecret::new("project-secret".to_owned()),
        CredentialSecret::new("mongo-root".to_owned()),
    )
    .expect("MongoDB logical plan");

    assert_eq!(plan.database_name(), "stackctl_bill_database");
    assert_eq!(plan.username(), "st_bill_database");
    assert_eq!(plan.credential_id(), "bill/database/mongodb");
    assert_eq!(plan.command_arguments(), ["mongosh", "--quiet", "--nodb"]);
    assert!(plan.stdin_script().contains("getUser"));
    assert!(plan.stdin_script().contains("updateUser"));
    assert!(plan.stdin_script().contains("createUser"));
    assert!(plan.stdin_script().contains("readWrite"));
    assert!(plan.stdin_script().contains("project-secret"));
    assert!(plan.stdin_script().contains("mongo-root"));
    assert!(!format!("{plan:?}").contains("project-secret"));
    assert!(!format!("{plan:?}").contains("mongo-root"));
}

#[test]
fn rabbitmq_password_hashes_match_the_documented_salted_sha256_format() {
    let hash = RabbitMqPasswordHash::from_salt(
        CredentialSecret::new("project-secret".to_owned()),
        [1, 2, 3, 4],
    );

    assert_eq!(
        hash.encoded(),
        "AQIDBC+1G1c0sCkZnSk4UmOklfTffD0p+fnkx/QqloCOZ0es"
    );
    assert_eq!(format!("{hash:?}"), "RabbitMqPasswordHash([REDACTED])");
}

#[test]
fn rabbitmq_definitions_are_deterministic_isolated_and_hash_only() {
    let definitions = RabbitMqDefinitions::new(vec![
        RabbitMqProjectDefinition::new(
            "shop",
            "broker",
            CredentialSecret::new("shop-secret".to_owned()),
        )
        .expect("shop definition"),
        RabbitMqProjectDefinition::new(
            "bill",
            "broker",
            CredentialSecret::new("bill-secret".to_owned()),
        )
        .expect("bill definition"),
    ])
    .expect("RabbitMQ definitions");
    let document: serde_json::Value =
        serde_json::from_slice(definitions.contents()).expect("definitions JSON");

    assert_eq!(
        document["vhosts"],
        serde_json::json!([
            {"name": "stackctl_bill_broker"},
            {"name": "stackctl_shop_broker"}
        ])
    );
    assert_eq!(document["users"][0]["name"], "st_bill_broker");
    assert_eq!(
        document["users"][0]["hashing_algorithm"],
        "rabbit_password_hashing_sha256"
    );
    assert_eq!(document["users"][0]["tags"], serde_json::json!([]));
    assert_eq!(
        document["permissions"][0],
        serde_json::json!({
            "user": "st_bill_broker",
            "vhost": "stackctl_bill_broker",
            "configure": ".*",
            "write": ".*",
            "read": ".*"
        })
    );
    assert!(!String::from_utf8_lossy(definitions.contents()).contains("secret"));
    assert_eq!(
        format!("{definitions:?}"),
        "RabbitMqDefinitions { project_count: 2 }"
    );
}

#[test]
fn rabbitmq_definitions_reject_deterministic_identity_collisions() {
    let definitions = vec![
        RabbitMqProjectDefinition::new(
            "bill-api",
            "broker",
            CredentialSecret::new("one".to_owned()),
        )
        .expect("first definition"),
        RabbitMqProjectDefinition::new(
            "bill",
            "api-broker",
            CredentialSecret::new("two".to_owned()),
        )
        .expect("second definition"),
    ];

    let error = RabbitMqDefinitions::new(definitions).expect_err("identity collision");

    assert_eq!(
        error.to_string(),
        "RabbitMQ user 'st_bill_api_broker' is defined more than once"
    );
}

#[cfg(unix)]
#[test]
fn rabbitmq_definitions_store_is_private_hash_only_and_atomically_replaceable() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-rabbitmq-definitions-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    drop(std::fs::remove_dir_all(&root));
    let initial = RabbitMqDefinitions::new(Vec::new()).expect("initial definitions");
    let replacement = RabbitMqDefinitions::new(vec![
        RabbitMqProjectDefinition::new(
            "bill",
            "broker",
            CredentialSecret::new("project-secret".to_owned()),
        )
        .expect("project definition"),
    ])
    .expect("replacement definitions");

    let stored = store_rabbitmq_definitions(&initial, &root).expect("initial store");
    store_rabbitmq_definitions(&replacement, &root).expect("replacement store");

    assert_eq!(stored.directory(), root);
    assert_eq!(stored.mount_directory(), root.join("mounted"));
    assert_eq!(stored.config_file(), root.join("mounted/rabbitmq.conf"));
    assert_eq!(
        stored.definitions_file(),
        root.join("mounted/definitions.json")
    );
    assert_eq!(
        std::fs::read_to_string(stored.config_file()).expect("RabbitMQ config"),
        "definitions.import_backend = local_filesystem\n\
         definitions.local.path = /etc/stackctl/rabbitmq/definitions.json\n\
         definitions.skip_if_unchanged = true\n"
    );
    let contents =
        std::fs::read_to_string(stored.definitions_file()).expect("RabbitMQ definitions");
    assert_eq!(contents.as_bytes(), replacement.contents());
    assert!(!contents.contains("project-secret"));
    assert_eq!(
        std::fs::metadata(stored.directory())
            .expect("private root")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(stored.mount_directory())
            .expect("mount directory")
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    assert_eq!(
        std::fs::metadata(stored.definitions_file())
            .expect("definitions file")
            .permissions()
            .mode()
            & 0o777,
        0o644
    );

    std::fs::remove_dir_all(&root).expect("remove RabbitMQ fixture");
}

#[test]
fn rabbitmq_materializes_one_private_persistent_definition_backed_instance() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "broker",
        rabbitmq_profile("4"),
    )])
    .pop()
    .expect("shared RabbitMQ plan");
    let plan = RabbitMqSharedInstancePlan::new(
        &shared,
        RabbitMqSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:rabbitmq-v1".to_owned(),
            definitions_directory: "/private/rabbitmq/mounted".into(),
        },
    )
    .expect("RabbitMQ instance");

    assert!(plan.container().name().starts_with("stackctl-shared-"));
    assert_eq!(plan.container().image(), shared.profile().image_digest());
    assert_eq!(plan.config_mount_target(), "/etc/stackctl/rabbitmq");
    assert_eq!(plan.data_mount_target(), "/var/lib/rabbitmq");
    assert_eq!(plan.config_file(), "/etc/stackctl/rabbitmq/rabbitmq.conf");
    assert_eq!(plan.node_name(), "rabbit@localhost");
    assert!(plan.volume().is_some());
    let debug = format!("{:?}", plan.container());
    assert!(debug.contains("/private/rabbitmq/mounted"));
    assert!(debug.contains("read_only: true"));
    assert!(debug.contains("RABBITMQ_CONFIG_FILE"));
    assert!(debug.contains("RABBITMQ_NODENAME"));
}

#[test]
fn rabbitmq_project_resources_compose_vhost_credential_and_environment() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "broker",
        rabbitmq_profile("4"),
    )])
    .pop()
    .expect("shared RabbitMQ plan");
    let instance = RabbitMqSharedInstancePlan::new(
        &shared,
        RabbitMqSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:rabbitmq-v1".to_owned(),
            definitions_directory: "/private/rabbitmq/mounted".into(),
        },
    )
    .expect("RabbitMQ instance");

    let project = plan_rabbitmq_project_resources(
        "bill",
        "broker",
        &instance,
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("RabbitMQ project resources");

    assert_eq!(project.definition().username(), "st_bill_broker");
    assert_eq!(project.definition().vhost(), "stackctl_bill_broker");
    assert_eq!(project.credential().credential_id(), "bill/broker/rabbitmq");
    assert_eq!(project.credential().secret(), "project-secret");
    assert_eq!(
        project.environment().values(),
        &BTreeMap::from([
            (
                "RABBITMQ_HOST".to_owned(),
                instance.container().name().to_owned()
            ),
            ("RABBITMQ_PASSWORD".to_owned(), "project-secret".to_owned()),
            ("RABBITMQ_PORT".to_owned(), "5672".to_owned()),
            ("RABBITMQ_USERNAME".to_owned(), "st_bill_broker".to_owned()),
            (
                "RABBITMQ_VHOST".to_owned(),
                "stackctl_bill_broker".to_owned()
            ),
        ])
    );
    assert!(!format!("{project:?}").contains("project-secret"));
}

#[test]
fn rabbitmq_live_definitions_import_is_bounded_and_credential_free() {
    let shared = plan_shared_instances(vec![SharedServiceRequest::new(
        "bill",
        "broker",
        rabbitmq_profile("4"),
    )])
    .pop()
    .expect("shared RabbitMQ plan");
    let instance = RabbitMqSharedInstancePlan::new(
        &shared,
        RabbitMqSharedInstancePlanOptions {
            installation_id: "install-1".to_owned(),
            network_name: "stackctl".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:rabbitmq-v1".to_owned(),
            definitions_directory: "/private/rabbitmq/mounted".into(),
        },
    )
    .expect("RabbitMQ instance");
    let container = owned_shared_container("rabbitmq-container", "sha256:rabbitmq-4");
    let executor = RecordingPostgresExecutor::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    runtime
        .block_on(reload_rabbitmq_definitions(
            &executor, &container, &instance,
        ))
        .expect("reload RabbitMQ definitions");
    runtime.block_on(tokio::task::yield_now());

    let request_debug = executor
        .request_debug
        .lock()
        .expect("recorded request")
        .clone();
    assert!(request_debug.contains("argument_count: 3"));
    assert!(request_debug.contains("environment_keys: []"));
    assert!(executor.stdin.lock().expect("recorded stdin").is_empty());
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
         user st_bill_cache on resetpass #fdb34f0710b2f482f4eb9dded04a6777f64c7388e42b0553ad43b26e126b029c resetkeys ~stackctl:bill:cache:* resetchannels &stackctl:bill:cache:* -@all +@read +@write +@connection +@transaction +@pubsub +@scripting -@admin -@dangerous\n\
         user st_shop_cache on resetpass #3c655a3878fd8e4145a5facca30188ce74792ddff5d57aaf2203bbe74a940cb5 resetkeys ~stackctl:shop:cache:* resetchannels &stackctl:shop:cache:* -@all +@read +@write +@connection +@transaction +@pubsub +@scripting -@admin -@dangerous\n"
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
fn redis_acl_reload_uses_environment_auth_and_checks_command_status() {
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
    let container = owned_shared_container("redis-container", "sha256:redis-8");
    let executor = RecordingPostgresExecutor::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    runtime
        .block_on(reload_redis_acl(&executor, &container, &instance))
        .expect("reload Redis ACL");
    runtime.block_on(tokio::task::yield_now());

    let request_debug = executor
        .request_debug
        .lock()
        .expect("recorded request")
        .clone();
    assert!(request_debug.contains("argument_count: 6"));
    assert!(request_debug.contains("REDISCLI_AUTH"));
    assert!(!request_debug.contains("redis-admin"));
    assert!(executor.stdin.lock().expect("recorded stdin").is_empty());
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

fn rabbitmq_profile(major_version: &str) -> CompatibilityProfile {
    CompatibilityProfile::from_options(CompatibilityFingerprintOptions {
        implementation: "rabbitmq".to_owned(),
        major_version: major_version.to_owned(),
        image_digest: format!("rabbitmq@sha256:{}", "d".repeat(64)),
        extensions: Vec::new(),
        immutable_settings: BTreeMap::new(),
        persistence: PersistenceMode::Persistent,
        isolation: IsolationCapability::VirtualHostAndUser,
        platform_architecture: Some("linux/arm64".to_owned()),
    })
    .expect("valid RabbitMQ compatibility profile")
}

fn mongodb_profile(major_version: &str) -> CompatibilityProfile {
    CompatibilityProfile::from_options(CompatibilityFingerprintOptions {
        implementation: "mongodb".to_owned(),
        major_version: major_version.to_owned(),
        image_digest: format!("mongo@sha256:{}", "e".repeat(64)),
        extensions: Vec::new(),
        immutable_settings: BTreeMap::new(),
        persistence: PersistenceMode::Persistent,
        isolation: IsolationCapability::DatabaseAndRole,
        platform_architecture: Some("linux/arm64".to_owned()),
    })
    .expect("valid MongoDB compatibility profile")
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
