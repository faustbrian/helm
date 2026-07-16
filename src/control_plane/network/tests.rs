use super::{
    NetworkReconcileAction, NetworkReconcileOptions, NetworksReconcileOptions,
    StaleProjectNetworkCleanupOptions, cleanup_stale_project_networks, global_network_request,
    matches_global_network, project_network_request, reconcile_network, reconcile_networks,
    stale_project_networks,
};

#[test]
fn production_global_network_request_has_one_deterministic_identity() {
    let request = global_network_request("install-1").expect("global network request");

    assert_eq!(request.name(), "stackctl");
    assert_eq!(request.metadata().installation_id(), "install-1");
    assert_eq!(request.metadata().kind(), ResourceKind::Network);
    assert_eq!(request.metadata().project_id(), None);
    assert_eq!(request.metadata().resource_id(), Some("private"));
    assert_eq!(request.metadata().compatibility_fingerprint(), "network-v1");
    assert_eq!(
        request
            .metadata()
            .labels()
            .get("dev.stackctl.desired")
            .map(String::as_str),
        Some("network-v1")
    );
    assert_eq!(request.metadata().retention(), RetentionClass::Persistent);
}

#[test]
fn project_network_request_has_one_deterministic_owned_identity() {
    let request =
        project_network_request("install-1", "stackctl", "bill").expect("project network request");

    assert_eq!(request.name(), "stackctl-bill");
    assert_eq!(request.metadata().installation_id(), "install-1");
    assert_eq!(request.metadata().kind(), ResourceKind::Network);
    assert_eq!(request.metadata().project_id(), Some("bill"));
    assert_eq!(request.metadata().resource_id(), Some("private"));
    assert_eq!(request.metadata().compatibility_fingerprint(), "network-v1");
    assert_eq!(request.metadata().retention(), RetentionClass::Persistent);
    assert_eq!(request.subnet(), Some("10.154.51.0/24"));
    assert_eq!(
        project_network_request("install-1", "stackctl", "bill")
            .expect("repeated project network request")
            .subnet(),
        request.subnet()
    );
}

#[test]
fn global_network_match_requires_the_complete_canonical_identity() {
    let request = global_network_request("install-1").expect("global network request");
    let exact = reconstruct_owned_network(
        &ObservedNetwork::new(NetworkId::new("network-1"), request.metadata().labels()),
        "install-1",
        8,
    )
    .expect("owned global network");
    let mut wrong_labels = request.metadata().labels();
    wrong_labels.insert("dev.stackctl.desired".to_owned(), "network-v2".to_owned());
    let wrong = reconstruct_owned_network(
        &ObservedNetwork::new(NetworkId::new("network-2"), wrong_labels),
        "install-1",
        8,
    )
    .expect("owned noncanonical network");

    assert!(matches_global_network(&exact, "install-1"));
    assert!(!matches_global_network(&wrong, "install-1"));
    assert!(!matches_global_network(&exact, "install-2"));
}
use crate::control_plane::engine::{
    ContainerId, ContainerNetworkIsolation, EngineError, EngineFuture, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, NetworkCreateOptions, NetworkDiscovery, NetworkId,
    NetworkManager, ObservedContainer, ObservedNetwork, OwnedContainer, OwnedNetwork, ResourceKind,
    RetentionClass, reconstruct_owned_container, reconstruct_owned_network,
};
use std::collections::BTreeSet;

#[test]
fn global_network_is_created_once_and_then_adopted_by_exact_ownership() {
    let request = test_global_network_request();
    let mut engine = RecordingNetworkEngine::default();
    let runtime = runtime();

    let created = runtime
        .block_on(reconcile_network(
            &mut engine,
            NetworkReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("create global network");

    assert_eq!(created.action(), NetworkReconcileAction::Created);
    assert_eq!(engine.created, vec![request.clone()]);

    engine.observed = vec![ObservedNetwork::new(
        created.network().id().clone(),
        created.network().metadata().labels(),
    )];
    let adopted = runtime
        .block_on(reconcile_network(
            &mut engine,
            NetworkReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("adopt global network");

    assert_eq!(adopted.action(), NetworkReconcileAction::Unchanged);
    assert_eq!(adopted.network(), created.network());
    assert_eq!(engine.created.len(), 1);
}

#[test]
fn global_network_reconciliation_ignores_owned_project_networks() {
    let request = test_global_network_request();
    let project =
        project_network_request("install-1", "stackctl", "bill").expect("project network request");
    let mut engine = RecordingNetworkEngine {
        observed: vec![
            ObservedNetwork::new(NetworkId::new("global"), request.metadata().labels()),
            ObservedNetwork::new(NetworkId::new("project"), project.metadata().labels()),
        ],
        ..RecordingNetworkEngine::default()
    };

    let result = runtime()
        .block_on(reconcile_network(
            &mut engine,
            NetworkReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("adopt global network beside project networks");

    assert_eq!(result.network().id().as_str(), "global");
    assert_eq!(result.action(), NetworkReconcileAction::Unchanged);
    assert!(engine.created.is_empty());
}

#[test]
fn project_network_is_created_beside_the_global_network() {
    let global = test_global_network_request();
    let project =
        project_network_request("install-1", "stackctl", "bill").expect("project network request");
    let mut engine = RecordingNetworkEngine {
        observed: vec![ObservedNetwork::new(
            NetworkId::new("global"),
            global.metadata().labels(),
        )],
        ..RecordingNetworkEngine::default()
    };

    let result = runtime()
        .block_on(reconcile_network(
            &mut engine,
            NetworkReconcileOptions {
                request: &project,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("create project network");

    assert_eq!(result.action(), NetworkReconcileAction::Created);
    assert_eq!(engine.created, vec![project]);
}

#[test]
fn complete_network_set_reuses_one_engine_discovery() {
    let requests = vec![
        global_network_request("install-1").expect("global network request"),
        project_network_request("install-1", "stackctl", "bill").expect("bill network request"),
        project_network_request("install-1", "stackctl", "ship").expect("ship network request"),
    ];
    let mut engine = RecordingNetworkEngine::default();

    let results = runtime()
        .block_on(reconcile_networks(
            &mut engine,
            NetworksReconcileOptions {
                requests: &requests,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("reconcile complete network set");

    assert_eq!(results.len(), 3);
    assert_eq!(engine.created, requests);
    assert_eq!(
        engine
            .discovery_count
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
}

#[test]
fn stale_project_network_inventory_selects_only_absent_projects() {
    let global = global_network_request("install-1").expect("global network request");
    let bill =
        project_network_request("install-1", "stackctl", "bill").expect("bill network request");
    let ship =
        project_network_request("install-1", "stackctl", "ship").expect("ship network request");
    let observed = vec![
        ObservedNetwork::new(NetworkId::new("global"), global.metadata().labels()),
        ObservedNetwork::new(NetworkId::new("bill"), bill.metadata().labels()),
        ObservedNetwork::new(NetworkId::new("ship"), ship.metadata().labels()),
    ];

    let stale = stale_project_networks(
        &observed,
        "install-1",
        8,
        &BTreeSet::from(["bill".to_owned()]),
    )
    .expect("stale project network inventory");

    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].id().as_str(), "ship");
}

#[test]
fn stale_project_network_cleanup_detaches_owned_consumers_before_removal() {
    let bill =
        project_network_request("install-1", "stackctl", "bill").expect("bill network request");
    let observed_networks = vec![ObservedNetwork::new(
        NetworkId::new("bill-network"),
        bill.metadata().labels(),
    )];
    let shared = test_owned_container("shared", ResourceKind::SharedService, None);
    let application =
        test_owned_container("bill-app", ResourceKind::ProjectApplication, Some("bill"));
    let gateway = test_owned_container("gateway", ResourceKind::Gateway, None);
    let observed_containers = [&shared, &application]
        .into_iter()
        .map(|container| {
            ObservedContainer::new(
                container.id().clone(),
                container.metadata().labels().into_iter().collect(),
            )
        })
        .collect::<Vec<_>>();
    let mut engine = RecordingNetworkEngine::default();

    let removed = runtime()
        .block_on(cleanup_stale_project_networks(
            &mut engine,
            StaleProjectNetworkCleanupOptions {
                observed_networks: &observed_networks,
                observed_containers: &observed_containers,
                gateway: &gateway,
                active_project_ids: &BTreeSet::new(),
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("clean stale project network");

    assert_eq!(removed, 1);
    let mut disconnected = engine
        .disconnected
        .lock()
        .expect("disconnected network record")
        .clone();
    disconnected.sort();
    assert_eq!(
        disconnected,
        vec![
            ("bill-app".to_owned(), "bill-network".to_owned()),
            ("gateway".to_owned(), "bill-network".to_owned()),
            ("shared".to_owned(), "bill-network".to_owned()),
        ]
    );
    assert_eq!(engine.removed, vec!["bill-network"]);
}

fn test_owned_container(id: &str, kind: ResourceKind, project_id: Option<&str>) -> OwnedContainer {
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind,
        project_id: project_id.map(str::to_owned),
        compatibility_fingerprint: "test-v1".to_owned(),
        schema_version: 8,
        desired_revision: "test-v1".to_owned(),
        retention: RetentionClass::Disposable,
    })
    .expect("container metadata")
    .with_resource_id("resource")
    .expect("container resource identity");

    reconstruct_owned_container(
        &ObservedContainer::new(
            ContainerId::new(id),
            metadata.labels().into_iter().collect(),
        ),
        "install-1",
        8,
    )
    .expect("owned container")
}

#[test]
fn global_network_reconciliation_refuses_duplicate_or_mismatched_ownership() {
    let request = test_global_network_request();
    let metadata = request.metadata().clone();
    let mut engine = RecordingNetworkEngine {
        observed: vec![
            ObservedNetwork::new(NetworkId::new("network-1"), metadata.labels()),
            ObservedNetwork::new(NetworkId::new("network-2"), metadata.labels()),
        ],
        ..RecordingNetworkEngine::default()
    };
    let runtime = runtime();

    let duplicate = runtime
        .block_on(reconcile_network(
            &mut engine,
            NetworkReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect_err("duplicate global networks must conflict");

    assert!(duplicate.to_string().contains("observed 2 times"));
    assert!(engine.created.is_empty());

    let mismatched = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::Network,
        project_id: None,
        compatibility_fingerprint: "network-v2".to_owned(),
        schema_version: 8,
        desired_revision: "network-v2".to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("mismatched metadata")
    .with_resource_id("private")
    .expect("resource identity");
    engine.observed = vec![ObservedNetwork::new(
        NetworkId::new("network-1"),
        mismatched.labels(),
    )];

    let mismatch = runtime
        .block_on(reconcile_network(
            &mut engine,
            NetworkReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect_err("mismatched global network must conflict");

    assert!(mismatch.to_string().contains("does not match"));
    assert!(engine.created.is_empty());
}

fn test_global_network_request() -> NetworkCreateOptions {
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::Network,
        project_id: None,
        compatibility_fingerprint: "network-v1".to_owned(),
        schema_version: 8,
        desired_revision: "network-v1".to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("global network metadata")
    .with_resource_id("private")
    .expect("global network identity");

    NetworkCreateOptions::new("stackctl", metadata).expect("global network request")
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime")
}

#[derive(Default)]
struct RecordingNetworkEngine {
    observed: Vec<ObservedNetwork>,
    created: Vec<NetworkCreateOptions>,
    disconnected: std::sync::Mutex<Vec<(String, String)>>,
    removed: Vec<String>,
    discovery_count: std::sync::atomic::AtomicUsize,
}

impl NetworkDiscovery for RecordingNetworkEngine {
    fn discover_managed_networks(&self) -> EngineFuture<'_, Vec<ObservedNetwork>> {
        self.discovery_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async { Ok(self.observed.clone()) })
    }
}

impl NetworkManager for RecordingNetworkEngine {
    fn create_network<'operation>(
        &'operation mut self,
        options: &'operation NetworkCreateOptions,
    ) -> EngineFuture<'operation, OwnedNetwork> {
        Box::pin(async move {
            self.created.push(options.clone());

            reconstruct_owned_network(
                &ObservedNetwork::new(NetworkId::new("network-1"), options.metadata().labels()),
                options.metadata().installation_id(),
                options.metadata().schema_version(),
            )
            .map_err(|ownership| EngineError::Backend {
                detail: format!("test ownership reconstruction failed: {ownership:?}"),
            })
        })
    }

    fn remove_network<'operation>(
        &'operation mut self,
        network: &'operation OwnedNetwork,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.removed.push(network.id().as_str().to_owned());
            Ok(())
        })
    }
}

impl ContainerNetworkIsolation for RecordingNetworkEngine {
    fn disconnect_container_network<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        network: &'operation OwnedNetwork,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.disconnected
                .lock()
                .expect("disconnected network record")
                .push((
                    container.id().as_str().to_owned(),
                    network.id().as_str().to_owned(),
                ));
            Ok(())
        })
    }

    fn reconnect_container_network<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
        _network: &'operation OwnedNetwork,
        _alias: &'operation str,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async {
            Err(EngineError::Backend {
                detail: "stale cleanup must not reconnect containers".to_owned(),
            })
        })
    }
}
