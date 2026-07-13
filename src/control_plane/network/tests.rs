use super::{
    GlobalNetworkReconcileAction, GlobalNetworkReconcileOptions, global_network_request,
    reconcile_global_network,
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
use crate::control_plane::engine::{
    EngineError, EngineFuture, ManagedResourceMetadata, ManagedResourceMetadataOptions,
    NetworkCreateOptions, NetworkDiscovery, NetworkId, NetworkManager, ObservedNetwork,
    OwnedNetwork, ResourceKind, RetentionClass, reconstruct_owned_network,
};

#[test]
fn global_network_is_created_once_and_then_adopted_by_exact_ownership() {
    let request = test_global_network_request();
    let mut engine = RecordingNetworkEngine::default();
    let runtime = runtime();

    let created = runtime
        .block_on(reconcile_global_network(
            &mut engine,
            GlobalNetworkReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("create global network");

    assert_eq!(created.action(), GlobalNetworkReconcileAction::Created);
    assert_eq!(engine.created, vec![request.clone()]);

    engine.observed = vec![ObservedNetwork::new(
        created.network().id().clone(),
        created.network().metadata().labels(),
    )];
    let adopted = runtime
        .block_on(reconcile_global_network(
            &mut engine,
            GlobalNetworkReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
            },
        ))
        .expect("adopt global network");

    assert_eq!(adopted.action(), GlobalNetworkReconcileAction::Unchanged);
    assert_eq!(adopted.network(), created.network());
    assert_eq!(engine.created.len(), 1);
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
        .block_on(reconcile_global_network(
            &mut engine,
            GlobalNetworkReconcileOptions {
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
        .block_on(reconcile_global_network(
            &mut engine,
            GlobalNetworkReconcileOptions {
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
}

impl NetworkDiscovery for RecordingNetworkEngine {
    fn discover_managed_networks(&self) -> EngineFuture<'_, Vec<ObservedNetwork>> {
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
        _network: &'operation OwnedNetwork,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async {
            Err(EngineError::Backend {
                detail: "global network reconciliation must not remove networks".to_owned(),
            })
        })
    }
}
