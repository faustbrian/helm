use super::prepare_project_services;
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
    assert_eq!(first.credential().secret(), replayed.credential().secret());
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
        Some(&first.credential().secret().to_owned())
    );
    assert_eq!(
        first.environment().values().get("PUSHER_HOST"),
        Some(&"stackctl-bill-websocket".to_owned())
    );
    assert_eq!(
        first.environment().values().get("VITE_PUSHER_HOST"),
        Some(&"bill-websocket.stackctl.localhost".to_owned())
    );
    assert!(!format!("{first:?}").contains(first.credential().secret()));

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
    assert_eq!(
        first[0].credential().secret(),
        replayed[0].credential().secret()
    );
    assert_eq!(
        first[0].container_environment().get("TYPESENSE_DATA_DIR"),
        Some(&"/data".to_owned())
    );
    assert_eq!(
        first[0].container_environment().get("TYPESENSE_API_KEY"),
        Some(&first[0].credential().secret().to_owned())
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
    assert_eq!(
        first[0].credential().secret(),
        replayed[0].credential().secret()
    );
    assert_eq!(
        first[0].container_environment().get("MEILI_MASTER_KEY"),
        Some(&first[0].credential().secret().to_owned())
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
        Some(&first[0].credential().secret().to_owned())
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
    let password = first[0].credential().secret();
    assert_eq!(password, replayed[0].credential().secret());
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

struct FixedEntropy(u8);

impl CredentialEntropy for FixedEntropy {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), CredentialGenerationError> {
        bytes.fill(self.0);

        Ok(())
    }
}
