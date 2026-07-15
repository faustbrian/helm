use super::{materialize_project_service_configurations, prepare_project_services};
use crate::control_plane::application::{ProjectSource, plan_project_registry};
use crate::control_plane::resolve_execution_plan;
use crate::control_plane::shared_infrastructure::{CredentialEntropy, CredentialGenerationError};
use crate::control_plane::state::SqliteStateStore;
use crate::control_plane::state::StateStore;
use std::path::PathBuf;

#[test]
fn soketi_preparation_replays_one_stable_secret_and_complete_route_contract() {
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  websocket:\n    preset: soketi\n    version: '1'\n    image: quay.io/soketi/soketi@sha256:{}\n",
            "a".repeat(64)
        ),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let database = std::env::temp_dir().join(format!(
        "stackctl-soketi-preparation-{}-{}.sqlite3",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let mut store = SqliteStateStore::open(&database).expect("state store");

    let first = prepare_project_services(&mut store, &execution, &FixedEntropy(0x11))
        .expect("first preparation");
    let replayed = prepare_project_services(&mut store, &execution, &FixedEntropy(0x22))
        .expect("replayed preparation");

    assert_eq!(first.len(), 1);
    assert_eq!(replayed.len(), 1);
    let first = &first[0];
    let replayed = &replayed[0];
    let first_credential = first.credential().expect("Soketi credential");
    let replayed_credential = replayed.credential().expect("Soketi credential");
    assert_eq!(first_credential.secret(), replayed_credential.secret());
    assert_eq!(first.project_id(), "bill");
    assert_eq!(first.service_id(), "websocket");
    assert_eq!(
        first.route().expect("Soketi route").domain(),
        "bill-websocket.stackctl.localhost"
    );
    assert_eq!(
        first.route().expect("Soketi route").upstream(),
        "http://stackctl-bill-websocket:6001"
    );
    assert_eq!(
        first
            .container_environment()
            .get("SOKETI_DEFAULT_APP_SECRET"),
        Some(&first_credential.secret().to_owned())
    );
    assert_eq!(
        first.environment().values().get("PUSHER_HOST"),
        Some(&"stackctl-bill-websocket".to_owned())
    );
    assert_eq!(
        first.environment().values().get("VITE_PUSHER_HOST"),
        Some(&"bill-websocket.stackctl.localhost".to_owned())
    );
    assert!(!format!("{first:?}").contains(first_credential.secret()));

    std::fs::remove_file(database).expect("remove state store");
}

#[test]
fn soketi_preparation_rejects_reserved_environment_before_storing_a_secret() {
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            concat!(
                "schema_version: 8\nproject: bill\nservices:\n  websocket:\n",
                "    preset: soketi\n    version: '1'\n",
                "    image: quay.io/soketi/soketi@sha256:{}\n",
                "    environment:\n      SOKETI_DEFAULT_APP_SECRET: override\n"
            ),
            "a".repeat(64)
        ),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let database = std::env::temp_dir().join(format!(
        "stackctl-soketi-conflict-{}.sqlite3",
        std::process::id()
    ));
    let mut store = SqliteStateStore::open(&database).expect("state store");

    assert_eq!(
        prepare_project_services(&mut store, &execution, &FixedEntropy(0x11))
            .expect_err("reserved environment conflict")
            .to_string(),
        "Soketi service 'bill-websocket' cannot replace generated environment key \
         'SOKETI_DEFAULT_APP_SECRET'"
    );
    assert!(store.credentials().expect("credentials").is_empty());

    std::fs::remove_file(database).expect("remove state store");
}

#[test]
fn typesense_preparation_replays_stable_bootstrap_credentials_and_endpoints() {
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  search:\n    preset: typesense\n    version: '0'\n    image: typesense/typesense@sha256:{}\n",
            "b".repeat(64)
        ),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let database = std::env::temp_dir().join(format!(
        "stackctl-typesense-preparation-{}.sqlite3",
        std::process::id()
    ));
    let mut store = SqliteStateStore::open(&database).expect("state store");

    let first = prepare_project_services(&mut store, &execution, &FixedEntropy(0x33))
        .expect("first preparation");
    let replayed = prepare_project_services(&mut store, &execution, &FixedEntropy(0x44))
        .expect("replayed preparation");

    assert_eq!(first.len(), 1);
    let first_credential = first[0].credential().expect("Typesense credential");
    let replayed_credential = replayed[0].credential().expect("Typesense credential");
    assert_eq!(first_credential.secret(), replayed_credential.secret());
    assert_eq!(
        first[0].container_environment().get("TYPESENSE_DATA_DIR"),
        Some(&"/data".to_owned())
    );
    assert_eq!(
        first[0].container_environment().get("TYPESENSE_API_KEY"),
        Some(&first_credential.secret().to_owned())
    );
    assert_eq!(
        first[0].environment().values().get("TYPESENSE_HOST"),
        Some(&"stackctl-bill-search".to_owned())
    );
    assert_eq!(
        first[0].environment().values().get("TYPESENSE_PORT"),
        Some(&"8108".to_owned())
    );
    assert_eq!(first[0].route(), None);

    std::fs::remove_file(database).expect("remove state store");
}

#[test]
fn meilisearch_preparation_replays_stable_master_key_and_private_endpoint() {
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  search:\n    preset: meilisearch\n    version: '1'\n    image: getmeili/meilisearch@sha256:{}\n",
            "c".repeat(64)
        ),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let database = std::env::temp_dir().join(format!(
        "stackctl-meilisearch-preparation-{}.sqlite3",
        std::process::id()
    ));
    let mut store = SqliteStateStore::open(&database).expect("state store");

    let first = prepare_project_services(&mut store, &execution, &FixedEntropy(0x55))
        .expect("first preparation");
    let replayed = prepare_project_services(&mut store, &execution, &FixedEntropy(0x66))
        .expect("replayed preparation");

    assert_eq!(first.len(), 1);
    let first_credential = first[0].credential().expect("Meilisearch credential");
    let replayed_credential = replayed[0].credential().expect("Meilisearch credential");
    assert_eq!(first_credential.secret(), replayed_credential.secret());
    assert_eq!(
        first[0].container_environment().get("MEILI_MASTER_KEY"),
        Some(&first_credential.secret().to_owned())
    );
    assert_eq!(
        first[0].container_environment().get("MEILI_DB_PATH"),
        Some(&"/meili_data".to_owned())
    );
    assert_eq!(
        first[0].environment().values().get("MEILISEARCH_HOST"),
        Some(&"http://stackctl-bill-search:7700".to_owned())
    );
    assert_eq!(
        first[0].environment().values().get("MEILISEARCH_KEY"),
        Some(&first_credential.secret().to_owned())
    );
    assert_eq!(first[0].route(), None);

    std::fs::remove_file(database).expect("remove state store");
}

#[test]
fn meilisearch_preparation_rejects_reserved_environment_before_storing_a_secret() {
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            concat!(
                "schema_version: 8\nproject: bill\nservices:\n  search:\n",
                "    preset: meilisearch\n    version: '1'\n",
                "    image: getmeili/meilisearch@sha256:{}\n",
                "    environment:\n      MEILI_MASTER_KEY: override\n"
            ),
            "c".repeat(64)
        ),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let database = std::env::temp_dir().join(format!(
        "stackctl-meilisearch-conflict-{}.sqlite3",
        std::process::id()
    ));
    let mut store = SqliteStateStore::open(&database).expect("state store");

    assert_eq!(
        prepare_project_services(&mut store, &execution, &FixedEntropy(0x55))
            .expect_err("reserved environment conflict")
            .to_string(),
        "Meilisearch service 'bill-search' cannot replace generated environment key \
         'MEILI_MASTER_KEY'"
    );
    assert!(store.credentials().expect("credentials").is_empty());

    std::fs::remove_file(database).expect("remove state store");
}

#[test]
fn opensearch_preparation_replays_a_policy_compatible_admin_identity() {
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  search:\n    preset: opensearch\n    version: '3'\n    image: opensearchproject/opensearch@sha256:{}\n",
            "d".repeat(64)
        ),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let database = std::env::temp_dir().join(format!(
        "stackctl-opensearch-preparation-{}.sqlite3",
        std::process::id()
    ));
    let mut store = SqliteStateStore::open(&database).expect("state store");

    let first = prepare_project_services(&mut store, &execution, &FixedEntropy(0x77))
        .expect("first preparation");
    let replayed = prepare_project_services(&mut store, &execution, &FixedEntropy(0x88))
        .expect("replayed preparation");

    assert_eq!(first.len(), 1);
    let password = first[0]
        .credential()
        .expect("OpenSearch credential")
        .secret();
    assert_eq!(
        password,
        replayed[0]
            .credential()
            .expect("OpenSearch credential")
            .secret()
    );
    assert!(password.starts_with("Aa1!"));
    assert!(password.len() >= 12);
    assert_eq!(
        first[0]
            .container_environment()
            .get("OPENSEARCH_INITIAL_ADMIN_PASSWORD"),
        Some(&password.to_owned())
    );
    assert_eq!(
        first[0].container_environment().get("discovery.type"),
        Some(&"single-node".to_owned())
    );
    assert_eq!(
        first[0].environment().values().get("OPENSEARCH_URL"),
        Some(&"https://stackctl-bill-search:9200".to_owned())
    );
    assert_eq!(
        first[0].environment().values().get("OPENSEARCH_USERNAME"),
        Some(&"admin".to_owned())
    );
    assert_eq!(
        first[0].environment().values().get("OPENSEARCH_PASSWORD"),
        Some(&password.to_owned())
    );

    std::fs::remove_file(database).expect("remove state store");
}

#[test]
fn opensearch_preparation_rejects_reserved_environment_before_storing_a_secret() {
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            concat!(
                "schema_version: 8\nproject: bill\nservices:\n  search:\n",
                "    preset: opensearch\n    version: '3'\n",
                "    image: opensearchproject/opensearch@sha256:{}\n",
                "    environment:\n      OPENSEARCH_INITIAL_ADMIN_PASSWORD: override\n"
            ),
            "d".repeat(64)
        ),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let database = std::env::temp_dir().join(format!(
        "stackctl-opensearch-conflict-{}.sqlite3",
        std::process::id()
    ));
    let mut store = SqliteStateStore::open(&database).expect("state store");

    assert_eq!(
        prepare_project_services(&mut store, &execution, &FixedEntropy(0x77))
            .expect_err("reserved environment conflict")
            .to_string(),
        "OpenSearch service 'bill-search' cannot replace generated environment key \
         'OPENSEARCH_INITIAL_ADMIN_PASSWORD'"
    );
    assert!(store.credentials().expect("credentials").is_empty());

    std::fs::remove_file(database).expect("remove state store");
}

#[test]
fn elasticsearch_preparation_replays_stable_credentials_and_private_http_endpoint() {
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  search:\n    preset: elasticsearch\n    version: '9'\n    image: docker.elastic.co/elasticsearch/elasticsearch@sha256:{}\n",
            "e".repeat(64)
        ),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let database = std::env::temp_dir().join(format!(
        "stackctl-elasticsearch-preparation-{}.sqlite3",
        std::process::id()
    ));
    let mut store = SqliteStateStore::open(&database).expect("state store");

    let first = prepare_project_services(&mut store, &execution, &FixedEntropy(0x99))
        .expect("first preparation");
    let replayed = prepare_project_services(&mut store, &execution, &FixedEntropy(0xaa))
        .expect("replayed preparation");

    assert_eq!(first.len(), 1);
    let first_credential = first[0].credential().expect("Elasticsearch credential");
    let replayed_credential = replayed[0].credential().expect("Elasticsearch credential");
    let password = first_credential.secret();
    assert_eq!(password, replayed_credential.secret());
    assert_eq!(first_credential.username(), "elastic");
    assert_eq!(
        first[0].container_environment().get("ELASTIC_PASSWORD"),
        Some(&password.to_owned())
    );
    assert_eq!(
        first[0].container_environment().get("discovery.type"),
        Some(&"single-node".to_owned())
    );
    assert_eq!(
        first[0]
            .container_environment()
            .get("xpack.security.enabled"),
        Some(&"true".to_owned())
    );
    assert_eq!(
        first[0]
            .container_environment()
            .get("xpack.security.autoconfiguration.enabled"),
        Some(&"false".to_owned())
    );
    assert_eq!(
        first[0]
            .container_environment()
            .get("xpack.security.http.ssl.enabled"),
        Some(&"false".to_owned())
    );
    assert_eq!(
        first[0].environment().values().get("ELASTICSEARCH_URL"),
        Some(&"http://stackctl-bill-search:9200".to_owned())
    );
    assert_eq!(
        first[0]
            .environment()
            .values()
            .get("ELASTICSEARCH_USERNAME"),
        Some(&"elastic".to_owned())
    );
    assert_eq!(
        first[0]
            .environment()
            .values()
            .get("ELASTICSEARCH_PASSWORD"),
        Some(&password.to_owned())
    );
    assert_eq!(first[0].route(), None);

    std::fs::remove_file(database).expect("remove state store");
}

#[test]
fn elasticsearch_preparation_rejects_reserved_environment_before_storing_a_secret() {
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            concat!(
                "schema_version: 8\nproject: bill\nservices:\n  search:\n",
                "    preset: elasticsearch\n    version: '9'\n",
                "    image: docker.elastic.co/elasticsearch/elasticsearch@sha256:{}\n",
                "    environment:\n      ELASTIC_PASSWORD: override\n"
            ),
            "e".repeat(64)
        ),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let database = std::env::temp_dir().join(format!(
        "stackctl-elasticsearch-conflict-{}.sqlite3",
        std::process::id()
    ));
    let mut store = SqliteStateStore::open(&database).expect("state store");

    assert_eq!(
        prepare_project_services(&mut store, &execution, &FixedEntropy(0x99))
            .expect_err("reserved environment conflict")
            .to_string(),
        "Elasticsearch service 'bill-search' cannot replace generated environment key \
         'ELASTIC_PASSWORD'"
    );
    assert!(store.credentials().expect("credentials").is_empty());

    std::fs::remove_file(database).expect("remove state store");
}

#[test]
fn memcached_preparation_injects_an_endpoint_without_creating_credentials() {
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  cache:\n    preset: memcached\n    version: '1'\n    image: memcached@sha256:{}\n",
            "f".repeat(64)
        ),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let database = std::env::temp_dir().join(format!(
        "stackctl-memcached-preparation-{}.sqlite3",
        std::process::id()
    ));
    let mut store = SqliteStateStore::open(&database).expect("state store");

    let first = prepare_project_services(&mut store, &execution, &FixedEntropy(0xbb))
        .expect("first preparation");
    let replayed = prepare_project_services(&mut store, &execution, &FixedEntropy(0xcc))
        .expect("replayed preparation");

    assert_eq!(first.len(), 1);
    assert_eq!(replayed.len(), 1);
    assert!(first[0].credential().is_none());
    assert!(store.credentials().expect("credentials").is_empty());
    assert!(first[0].container_environment().is_empty());
    assert_eq!(
        first[0].environment().values().get("MEMCACHED_HOST"),
        Some(&"stackctl-bill-cache".to_owned())
    );
    assert_eq!(
        first[0].environment().values().get("MEMCACHED_PORT"),
        Some(&"11211".to_owned())
    );
    assert_eq!(first[0].route(), None);

    std::fs::remove_file(database).expect("remove state store");
}

#[test]
fn localstack_preparation_enables_persistence_and_injects_sdk_defaults() {
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  aws:\n    preset: localstack\n    version: '4'\n    image: localstack/localstack@sha256:{}\n",
            "1".repeat(64)
        ),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let database = std::env::temp_dir().join(format!(
        "stackctl-localstack-preparation-{}.sqlite3",
        std::process::id()
    ));
    let mut store = SqliteStateStore::open(&database).expect("state store");

    let prepared = prepare_project_services(&mut store, &execution, &FixedEntropy(0xdd))
        .expect("LocalStack preparation");

    assert_eq!(prepared.len(), 1);
    assert!(prepared[0].credential().is_none());
    assert!(store.credentials().expect("credentials").is_empty());
    assert_eq!(
        prepared[0].container_environment().get("GATEWAY_LISTEN"),
        Some(&"0.0.0.0:4566".to_owned())
    );
    assert_eq!(
        prepared[0].container_environment().get("LOCALSTACK_HOST"),
        Some(&"stackctl-bill-aws:4566".to_owned())
    );
    assert_eq!(
        prepared[0].container_environment().get("PERSISTENCE"),
        Some(&"1".to_owned())
    );
    assert_eq!(
        prepared[0].environment().values().get("AWS_ENDPOINT_URL"),
        Some(&"http://stackctl-bill-aws:4566".to_owned())
    );
    assert_eq!(
        prepared[0].environment().values().get("AWS_ACCESS_KEY_ID"),
        Some(&"test".to_owned())
    );
    assert_eq!(
        prepared[0]
            .environment()
            .values()
            .get("AWS_SECRET_ACCESS_KEY"),
        Some(&"test".to_owned())
    );
    assert_eq!(
        prepared[0].environment().values().get("AWS_DEFAULT_REGION"),
        Some(&"us-east-1".to_owned())
    );
    assert_eq!(prepared[0].route(), None);

    std::fs::remove_file(database).expect("remove state store");
}

#[test]
fn dragonfly_preparation_replays_stable_password_and_snapshot_contract() {
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  cache:\n    preset: dragonfly\n    version: '1'\n    image: docker.dragonflydb.io/dragonflydb/dragonfly@sha256:{}\n",
            "2".repeat(64)
        ),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let database = std::env::temp_dir().join(format!(
        "stackctl-dragonfly-preparation-{}.sqlite3",
        std::process::id()
    ));
    let mut store = SqliteStateStore::open(&database).expect("state store");

    let first = prepare_project_services(&mut store, &execution, &FixedEntropy(0xee))
        .expect("first preparation");
    let replayed = prepare_project_services(&mut store, &execution, &FixedEntropy(0xff))
        .expect("replayed preparation");

    assert_eq!(first.len(), 1);
    assert_eq!(replayed.len(), 1);
    let first = &first[0];
    let replayed = &replayed[0];
    let credential = first.credential().expect("Dragonfly credential");
    assert_eq!(credential.username(), "default");
    assert_eq!(
        credential.secret(),
        replayed
            .credential()
            .expect("replayed Dragonfly credential")
            .secret()
    );
    assert_eq!(
        first.container_environment().get("DFLY_bind"),
        Some(&"0.0.0.0".to_owned())
    );
    assert_eq!(
        first.container_environment().get("DFLY_dir"),
        Some(&"/data".to_owned())
    );
    assert_eq!(
        first
            .container_environment()
            .get("DFLY_primary_port_http_enabled"),
        Some(&"false".to_owned())
    );
    assert_eq!(
        first.container_environment().get("DFLY_requirepass"),
        Some(&credential.secret().to_owned())
    );
    assert_eq!(
        first.container_environment().get("DFLY_snapshot_cron"),
        Some(&"* * * * *".to_owned())
    );
    assert_eq!(
        first.environment().values().get("DRAGONFLY_HOST"),
        Some(&"stackctl-bill-cache".to_owned())
    );
    assert_eq!(
        first.environment().values().get("DRAGONFLY_PORT"),
        Some(&"6379".to_owned())
    );
    assert_eq!(
        first.environment().values().get("DRAGONFLY_USERNAME"),
        Some(&"default".to_owned())
    );
    assert_eq!(
        first.environment().values().get("DRAGONFLY_PASSWORD"),
        Some(&credential.secret().to_owned())
    );
    assert_eq!(first.route(), None);
    assert!(!format!("{first:?}").contains(credential.secret()));

    std::fs::remove_file(database).expect("remove state store");
}

#[test]
fn dragonfly_preparation_rejects_reserved_environment_before_storing_a_secret() {
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            concat!(
                "schema_version: 8\nproject: bill\nservices:\n  cache:\n",
                "    preset: dragonfly\n    version: '1'\n",
                "    image: docker.dragonflydb.io/dragonflydb/dragonfly@sha256:{}\n",
                "    environment:\n      DFLY_requirepass: override\n"
            ),
            "2".repeat(64)
        ),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let database = std::env::temp_dir().join(format!(
        "stackctl-dragonfly-conflict-{}.sqlite3",
        std::process::id()
    ));
    let mut store = SqliteStateStore::open(&database).expect("state store");

    assert_eq!(
        prepare_project_services(&mut store, &execution, &FixedEntropy(0xee))
            .expect_err("reserved environment conflict")
            .to_string(),
        "Dragonfly service 'bill-cache' cannot replace generated environment key \
         'DFLY_requirepass'"
    );
    assert!(store.credentials().expect("credentials").is_empty());

    std::fs::remove_file(database).expect("remove state store");
}

#[cfg(unix)]
#[test]
fn garage_preparation_materializes_private_config_and_zero_touch_bucket() {
    use std::os::unix::fs::PermissionsExt as _;

    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  storage:\n    preset: garage\n    version: '2'\n    image: dxflrs/garage@sha256:{}\n",
            "3".repeat(64)
        ),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let root = std::env::temp_dir().join(format!(
        "stackctl-garage-preparation-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("Garage fixture root");
    let database = root.join("state.sqlite3");
    let mut store = SqliteStateStore::open(&database).expect("state store");

    let mut first = prepare_project_services(&mut store, &execution, &FixedEntropy(0x12))
        .expect("first preparation");
    materialize_project_service_configurations(&mut first, &root)
        .expect("materialized Garage configuration");
    let mut replayed = prepare_project_services(&mut store, &execution, &FixedEntropy(0x34))
        .expect("replayed preparation");
    materialize_project_service_configurations(&mut replayed, &root)
        .expect("replayed Garage configuration");

    assert_eq!(first.len(), 1);
    assert_eq!(replayed.len(), 1);
    let first = &first[0];
    let credential = first.credential().expect("Garage credential");
    assert!(credential.username().starts_with("GK"));
    assert_eq!(credential.username().len(), 34);
    assert_eq!(
        credential.secret(),
        replayed[0]
            .credential()
            .expect("replayed Garage credential")
            .secret()
    );
    assert_eq!(
        first.container_command(),
        Some(
            ["/garage", "server", "--single-node", "--default-bucket"]
                .map(str::to_owned)
                .as_slice()
        )
    );
    assert_eq!(
        first
            .container_environment()
            .get("GARAGE_DEFAULT_ACCESS_KEY"),
        Some(&credential.username().to_owned())
    );
    assert_eq!(
        first
            .container_environment()
            .get("GARAGE_DEFAULT_SECRET_KEY"),
        Some(&credential.secret().to_owned())
    );
    assert_eq!(
        first.container_environment().get("GARAGE_DEFAULT_BUCKET"),
        Some(&"stackctl-bill-storage".to_owned())
    );
    assert_eq!(
        first.environment().values().get("AWS_ENDPOINT"),
        Some(&"http://stackctl-bill-storage:3900".to_owned())
    );
    assert_eq!(
        first.environment().values().get("AWS_BUCKET"),
        Some(&"stackctl-bill-storage".to_owned())
    );
    let mount = first
        .container_configuration_mount()
        .expect("Garage config mount");
    assert_eq!(mount.target(), "/etc/garage.toml");
    assert!(mount.is_read_only());
    let configuration = std::fs::read_to_string(mount.source()).expect("Garage config");
    assert!(configuration.contains("metadata_dir = \"/var/lib/garage/meta\""));
    assert!(configuration.contains("data_dir = \"/var/lib/garage/data\""));
    assert!(configuration.contains("replication_factor = 1"));
    assert!(configuration.contains("rpc_public_addr = \"stackctl-bill-storage:3901\""));
    assert!(!format!("{first:?}").contains(credential.secret()));
    assert_eq!(
        std::fs::metadata(mount.source())
            .expect("Garage config metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );

    drop(store);
    std::fs::remove_dir_all(root).expect("remove Garage fixture");
}

#[test]
fn garage_preparation_rejects_generated_command_override_before_storing_a_secret() {
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            concat!(
                "schema_version: 8\nproject: bill\nservices:\n  storage:\n",
                "    preset: garage\n    version: '2'\n",
                "    image: dxflrs/garage@sha256:{}\n",
                "    command: [/garage, server]\n"
            ),
            "3".repeat(64)
        ),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let database = std::env::temp_dir().join(format!(
        "stackctl-garage-command-conflict-{}.sqlite3",
        std::process::id()
    ));
    let mut store = SqliteStateStore::open(&database).expect("state store");

    assert_eq!(
        prepare_project_services(&mut store, &execution, &FixedEntropy(0x12))
            .expect_err("generated command conflict")
            .to_string(),
        "Garage service 'bill-storage' cannot replace its generated command"
    );
    assert!(store.credentials().expect("credentials").is_empty());

    std::fs::remove_file(database).expect("remove state store");
}

#[test]
fn rustfs_preparation_replays_stable_root_credentials_and_private_endpoint() {
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            "schema_version: 8\nproject: bill\nservices:\n  storage:\n    preset: rustfs\n    version: '1'\n    image: rustfs/rustfs@sha256:{}\n",
            "4".repeat(64)
        ),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let database = std::env::temp_dir().join(format!(
        "stackctl-rustfs-preparation-{}.sqlite3",
        std::process::id()
    ));
    let mut store = SqliteStateStore::open(&database).expect("state store");

    let first = prepare_project_services(&mut store, &execution, &FixedEntropy(0x56))
        .expect("first preparation");
    let replayed = prepare_project_services(&mut store, &execution, &FixedEntropy(0x78))
        .expect("replayed preparation");

    assert_eq!(first.len(), 1);
    assert_eq!(replayed.len(), 1);
    let first = &first[0];
    let credential = first.credential().expect("RustFS credential");
    assert_eq!(credential.username(), "stackctl_admin");
    assert_eq!(
        credential.secret(),
        replayed[0]
            .credential()
            .expect("replayed RustFS credential")
            .secret()
    );
    assert_eq!(
        first.container_environment().get("RUSTFS_ADDRESS"),
        Some(&":9000".to_owned())
    );
    assert_eq!(
        first.container_environment().get("RUSTFS_ACCESS_KEY"),
        Some(&"stackctl_admin".to_owned())
    );
    assert_eq!(
        first.container_environment().get("RUSTFS_SECRET_KEY"),
        Some(&credential.secret().to_owned())
    );
    assert_eq!(
        first.container_environment().get("RUSTFS_CONSOLE_ENABLE"),
        Some(&"false".to_owned())
    );
    assert_eq!(
        first.container_environment().get("RUSTFS_VOLUMES"),
        Some(&"/data".to_owned())
    );
    assert_eq!(
        first.environment().values().get("AWS_ENDPOINT"),
        Some(&"http://stackctl-bill-storage:9000".to_owned())
    );
    assert_eq!(
        first.environment().values().get("AWS_ACCESS_KEY_ID"),
        Some(&"stackctl_admin".to_owned())
    );
    assert_eq!(
        first.environment().values().get("AWS_BUCKET"),
        Some(&"stackctl-bill-storage".to_owned())
    );
    assert_eq!(first.route(), None);
    assert!(!format!("{first:?}").contains(credential.secret()));

    std::fs::remove_file(database).expect("remove state store");
}

#[test]
fn rustfs_preparation_rejects_reserved_environment_before_storing_a_secret() {
    let source = ProjectSource::new(
        PathBuf::from("/work/bill"),
        PathBuf::from("/work/bill/.stackctl.yaml"),
        format!(
            concat!(
                "schema_version: 8\nproject: bill\nservices:\n  storage:\n",
                "    preset: rustfs\n    version: '1'\n",
                "    image: rustfs/rustfs@sha256:{}\n",
                "    environment:\n      RUSTFS_SECRET_KEY: override\n"
            ),
            "4".repeat(64)
        ),
    );
    let registry = plan_project_registry(&[source]).expect("desired registry");
    let execution = resolve_execution_plan(&registry).expect("execution plan");
    let database = std::env::temp_dir().join(format!(
        "stackctl-rustfs-conflict-{}.sqlite3",
        std::process::id()
    ));
    let mut store = SqliteStateStore::open(&database).expect("state store");

    assert_eq!(
        prepare_project_services(&mut store, &execution, &FixedEntropy(0x56))
            .expect_err("reserved environment conflict")
            .to_string(),
        "RustFS service 'bill-storage' cannot replace generated environment key \
         'RUSTFS_SECRET_KEY'"
    );
    assert!(store.credentials().expect("credentials").is_empty());

    std::fs::remove_file(database).expect("remove state store");
}

struct FixedEntropy(u8);

impl CredentialEntropy for FixedEntropy {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), CredentialGenerationError> {
        bytes.fill(self.0);

        Ok(())
    }
}
