use super::{
    CaddyGatewayProvider, EngineGatewayPortProbe, GatewayConfiguration, GatewayConfigurationAction,
    GatewayDocumentLoader, GatewayError, GatewayFuture, GatewayPlaneOptions,
    GatewayPortAvailability, GatewayPortProbe, GatewayReadinessOptions, GatewayReconcileAction,
    GatewayReconcileOptions, GatewayRoute, GatewayRuntimeAssetOptions, GatewaySnapshot,
    GlobalGatewayRequestOptions, LocalhostResolver, SystemGatewayPortProbe, global_gateway_request,
    preflight_gateway_ports, prepare_gateway_runtime_assets, reconcile_gateway,
    reconcile_gateway_configuration, reconcile_gateway_plane, render_caddy_document,
    store_active_gateway_certificate_generation, store_caddy_bootstrap,
    verify_gateway_ports_available, verify_stackctl_localhost_resolution,
    wait_for_gateway_certificate_generation, wait_for_gateway_ready,
};
use crate::control_plane::engine::{
    ContainerCreateOptions, ContainerDiscovery, ContainerHealth, ContainerLifecycle,
    ContainerState, EngineError, EngineFuture, GatewayContainerRequestOptions, HealthObserver,
    ImageId, ImageResolver, ImmutableImageReference, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ObservedContainer, OwnedContainer, PublishedPortBinding,
    PublishedPortDiscovery, ResourceKind, RetentionClass, gateway_container_request,
    reconstruct_owned_container,
};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
#[test]
fn gateway_runtime_assets_recover_idempotently_without_exposing_the_ca_key() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-v8-gateway-assets-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos()
    ));
    let options = GatewayRuntimeAssetOptions {
        runtime_directory: &root,
        installation_id: "install-1",
        container_user: "501:20",
        now: time::macros::datetime!(2026-07-13 12:00 UTC),
    };

    let initial = prepare_gateway_runtime_assets(options).expect("prepare gateway assets");
    let interrupted_bootstrap = root.join("gateway/.config.tmp");
    std::fs::write(&interrupted_bootstrap, "partial gateway config")
        .expect("interrupted gateway bootstrap");
    let recovered = prepare_gateway_runtime_assets(options).expect("recover gateway assets");
    let renewed = prepare_gateway_runtime_assets(GatewayRuntimeAssetOptions {
        now: time::macros::datetime!(2026-10-20 12:00 UTC),
        ..options
    })
    .expect("renew expired gateway assets");

    assert_eq!(initial.request(), recovered.request());
    assert!(!initial.certificate_was_expired());
    assert!(renewed.certificate_was_expired());
    assert!(!interrupted_bootstrap.exists());
    assert_eq!(initial.bootstrap_paths(), recovered.bootstrap_paths());
    assert!(initial.bootstrap_paths().config_path().is_file());
    assert_eq!(
        initial.request().bind_mounts().len(),
        3,
        "leaf certificate, leaf key, and bootstrap only"
    );
    assert!(
        initial
            .request()
            .bind_mounts()
            .iter()
            .all(|mount| { !mount.source().ends_with("/ca.key") })
    );

    std::fs::remove_dir_all(root).expect("remove gateway asset fixture");
}

#[cfg(unix)]
#[test]
fn gateway_certificate_activation_is_published_atomically_and_waitable() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-v8-gateway-certificate-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos()
    ));
    let revision = "a".repeat(64);
    std::fs::create_dir_all(root.join("gateway")).expect("gateway directory");
    let interrupted_generation = root.join("gateway/.active-certificate-generation.tmp");
    std::fs::write(&interrupted_generation, "partial generation")
        .expect("interrupted generation publication");

    store_active_gateway_certificate_generation(&root, &revision)
        .expect("publish active gateway certificate");
    wait_for_gateway_certificate_generation(
        &root,
        &revision,
        Duration::from_millis(10),
        Duration::from_millis(1),
    )
    .expect("observe active gateway certificate");

    assert_eq!(
        std::fs::read_to_string(root.join("gateway/active-certificate-generation"))
            .expect("read active generation"),
        format!("{revision}\n")
    );
    assert!(!interrupted_generation.exists());

    std::fs::remove_dir_all(root).expect("remove gateway activation fixture");
}

#[test]
fn gateway_certificate_activation_wait_rejects_a_stale_generation() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-v8-gateway-certificate-stale-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos()
    ));
    let stale = "a".repeat(64);
    let expected = "b".repeat(64);
    store_active_gateway_certificate_generation(&root, &stale)
        .expect("publish stale gateway certificate");

    let error = wait_for_gateway_certificate_generation(
        &root,
        &expected,
        Duration::from_millis(2),
        Duration::from_millis(1),
    )
    .expect_err("stale gateway generation must time out");

    assert!(
        error
            .to_string()
            .contains("did not activate certificate generation")
    );

    std::fs::remove_dir_all(root).expect("remove stale activation fixture");
}

#[test]
fn production_gateway_request_pins_official_caddy_and_global_ownership() {
    let request = global_gateway_request(GlobalGatewayRequestOptions {
        installation_id: "install-1".to_owned(),
        container_user: "501:20".to_owned(),
        certificate_path: "/state/tls/bundle/wildcard.crt".into(),
        private_key_path: "/state/tls/bundle/wildcard.key".into(),
        certificate_revision: "bundle-v1".to_owned(),
        bootstrap_config_path: "/state/gateway/config.json".into(),
    })
    .expect("production gateway request");

    assert_eq!(request.name(), "stackctl-gateway");
    assert_eq!(
        request.image(),
        concat!(
            "caddy@sha256:",
            "af5fdcd76f2db5e4e974ee92f96ee8c0fc3edb55bd4ba5032547cbf3f65e486d"
        )
    );
    assert_eq!(request.network(), Some("stackctl"));
    assert_eq!(request.user(), Some("501:20"));
    assert_eq!(
        request.command(),
        ["caddy", "run", "--config", "/etc/stackctl/config.json"]
    );
    assert_eq!(request.metadata().installation_id(), "install-1");
    assert_eq!(request.metadata().kind(), ResourceKind::Gateway);
    assert_eq!(request.metadata().resource_id(), Some("gateway"));

    let renewed = global_gateway_request(GlobalGatewayRequestOptions {
        installation_id: "install-1".to_owned(),
        container_user: "501:20".to_owned(),
        certificate_path: "/state/tls/renewed/wildcard.crt".into(),
        private_key_path: "/state/tls/renewed/wildcard.key".into(),
        certificate_revision: "bundle-v2".to_owned(),
        bootstrap_config_path: "/state/gateway/config.json".into(),
    })
    .expect("renewed gateway request");
    assert_ne!(renewed.metadata(), request.metadata());
}

#[test]
fn live_gateway_acceptance_contract_covers_protocols_and_restart() {
    let script = include_str!("../../../scripts/accept-v8-gateway.sh");
    let fixture = include_str!("../../../acceptance/gateway/main.go");
    let workflow = include_str!("../../../.github/workflows/ci.yml");

    assert!(script.contains("af5fdcd76f2db5e4e974ee92f96ee8c0fc3edb55bd4ba5032547cbf3f65e486d"));
    for contract in [
        "http1",
        "http2",
        "redirect",
        "websocket",
        "streaming",
        "large_body",
        "graceful_reload",
        "restart",
    ] {
        assert!(
            fixture.contains(contract) || script.contains(contract),
            "gateway acceptance must cover {contract}"
        );
    }
    assert!(workflow.contains("Gateway Protocol Acceptance"));
    assert!(workflow.contains("platform: linux-x86_64"));
    assert!(workflow.contains("platform: linux-arm64"));
    assert!(workflow.contains("gateway-acceptance-${{ matrix.platform }}"));
    assert!(fixture.contains("continuity"));
    assert!(script.contains("continuity_probe"));
}

#[test]
fn complete_route_snapshots_are_sorted_before_provider_application() {
    let snapshot = GatewaySnapshot::new(vec![
        GatewayRoute::new("shop-mailpit.stackctl.localhost", "http://mailpit:8025")
            .expect("mail route"),
        GatewayRoute::new("shop-app.stackctl.localhost", "http://shop-app:8080")
            .expect("app route"),
    ])
    .expect("complete snapshot");

    assert_eq!(
        snapshot
            .routes()
            .iter()
            .map(|route| route.domain())
            .collect::<Vec<_>>(),
        vec![
            "shop-app.stackctl.localhost",
            "shop-mailpit.stackctl.localhost"
        ]
    );
    assert!(snapshot.revision().starts_with("sha256:"));
}

#[test]
fn complete_route_snapshot_revision_changes_with_its_routes() {
    let first = GatewaySnapshot::new(vec![
        GatewayRoute::new("shop-app.stackctl.localhost", "http://shop-app:8080")
            .expect("first route"),
    ])
    .expect("first snapshot");
    let second = GatewaySnapshot::new(vec![
        GatewayRoute::new("shop-app.stackctl.localhost", "http://shop-app-v2:8080")
            .expect("changed route"),
    ])
    .expect("second snapshot");

    assert_ne!(first.revision(), second.revision());
}

#[test]
fn duplicate_domains_reject_the_entire_gateway_snapshot() {
    let error = GatewaySnapshot::new(vec![
        GatewayRoute::new("shop-app.stackctl.localhost", "http://shop-app:8080")
            .expect("first route"),
        GatewayRoute::new("shop-app.stackctl.localhost", "http://other-app:8080")
            .expect("second route"),
    ])
    .expect_err("duplicate domain");

    assert_eq!(
        error.to_string(),
        "gateway domain 'shop-app.stackctl.localhost' has multiple upstreams"
    );
}

#[test]
fn gateway_upstreams_must_use_internal_plain_http() {
    let error = GatewayRoute::new("shop-app.stackctl.localhost", "https://shop-app:8443")
        .expect_err("TLS belongs at gateway");

    assert_eq!(
        error.to_string(),
        "gateway upstream 'https://shop-app:8443' must use internal plain HTTP"
    );
}

#[test]
fn stackctl_localhost_preflight_accepts_only_loopback_answers() {
    let resolver = RecordingLocalhostResolver::returning(vec![
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(Ipv6Addr::LOCALHOST),
    ]);

    verify_stackctl_localhost_resolution(&resolver).expect("loopback resolution");

    assert_eq!(
        resolver.hosts.borrow().as_slice(),
        &["stackctl-probe.stackctl.localhost"]
    );
}

#[test]
fn stackctl_localhost_preflight_rejects_non_loopback_answers() {
    let resolver =
        RecordingLocalhostResolver::returning(vec![IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10))]);

    let error = verify_stackctl_localhost_resolution(&resolver)
        .expect_err("non-loopback answer must fail closed");

    assert_eq!(
        error.to_string(),
        "stackctl-probe.stackctl.localhost resolved to non-loopback address 192.0.2.10"
    );
}

#[test]
fn gateway_port_preflight_reports_every_owner_and_never_selects_fallback_ports() {
    let probe = RecordingGatewayPortProbe::with_results([
        (
            SocketAddr::from((Ipv4Addr::LOCALHOST, 80)),
            GatewayPortAvailability::Occupied {
                owner: Some("container 'legacy-proxy'".to_owned()),
            },
        ),
        (
            SocketAddr::from((Ipv6Addr::LOCALHOST, 443)),
            GatewayPortAvailability::Occupied { owner: None },
        ),
    ]);

    let error = verify_gateway_ports_available(&probe).expect_err("occupied gateway ports");

    assert_eq!(
        error.to_string(),
        "gateway cannot bind required loopback ports:\n\
- 127.0.0.1:80 is occupied by container 'legacy-proxy'\n\
- [::1]:443 is occupied by an unknown host process or Engine binding\n\
stop or reconfigure each owner; Stackctl will not choose alternate ports"
    );
    assert_eq!(
        probe.addresses.borrow().as_slice(),
        &[
            SocketAddr::from((Ipv4Addr::LOCALHOST, 80)),
            SocketAddr::from((Ipv6Addr::LOCALHOST, 80)),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 443)),
            SocketAddr::from((Ipv6Addr::LOCALHOST, 443)),
        ]
    );
}

#[test]
fn system_gateway_port_probe_detects_a_loopback_listener_without_binding() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind loopback listener");
    let address = listener.local_addr().expect("listener address");

    assert_eq!(
        SystemGatewayPortProbe
            .probe(address)
            .expect("probe listener"),
        GatewayPortAvailability::Occupied { owner: None }
    );
}

#[test]
fn engine_gateway_port_probe_attributes_exact_and_wildcard_bindings() {
    let host_probe = RecordingGatewayPortProbe::with_results([(
        SocketAddr::from((Ipv6Addr::LOCALHOST, 80)),
        GatewayPortAvailability::Occupied { owner: None },
    )]);
    let bindings = vec![
        PublishedPortBinding::new(
            "container-1",
            "legacy-proxy",
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            80,
        )
        .expect("wildcard Engine binding"),
        PublishedPortBinding::new(
            "container-2",
            "secure-proxy",
            IpAddr::V6(Ipv6Addr::LOCALHOST),
            443,
        )
        .expect("exact Engine binding"),
    ];
    let probe = EngineGatewayPortProbe::new(&host_probe, &bindings);

    assert_eq!(
        probe
            .probe(SocketAddr::from((Ipv4Addr::LOCALHOST, 80)))
            .expect("IPv4 wildcard binding"),
        GatewayPortAvailability::Occupied {
            owner: Some("Engine container 'legacy-proxy' (id 'container-1')".to_owned())
        }
    );
    assert_eq!(
        probe
            .probe(SocketAddr::from((Ipv6Addr::LOCALHOST, 443)))
            .expect("exact IPv6 binding"),
        GatewayPortAvailability::Occupied {
            owner: Some("Engine container 'secure-proxy' (id 'container-2')".to_owned())
        }
    );
    assert_eq!(
        probe
            .probe(SocketAddr::from((Ipv6Addr::LOCALHOST, 80)))
            .expect("host listener fallback"),
        GatewayPortAvailability::Occupied { owner: None }
    );
    assert_eq!(
        host_probe.addresses.borrow().as_slice(),
        &[SocketAddr::from((Ipv6Addr::LOCALHOST, 80))]
    );
}

#[test]
fn gateway_port_preflight_discovers_engine_owners_before_host_probing() {
    let engine = RecordingPublishedPortDiscovery {
        bindings: vec![
            PublishedPortBinding::new(
                "container-1",
                "legacy-proxy",
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                80,
            )
            .expect("Engine binding"),
        ],
    };
    let host_probe = RecordingGatewayPortProbe::with_results([]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(preflight_gateway_ports(&engine, &host_probe))
        .expect_err("Engine port conflict");

    assert!(
        error
            .to_string()
            .contains("127.0.0.1:80 is occupied by Engine container 'legacy-proxy'")
    );
    assert!(
        !host_probe
            .addresses
            .borrow()
            .contains(&SocketAddr::from((Ipv4Addr::LOCALHOST, 80)))
    );
}

#[test]
fn gateway_reconciliation_creates_starts_and_observes_a_missing_gateway() {
    let request = gateway_request(gateway_metadata("sha256:gateway-v1"));
    let mut engine = RecordingGatewayEngine {
        health: ContainerHealth::Starting,
        ..RecordingGatewayEngine::default()
    };
    let host_probe = RecordingGatewayPortProbe::with_results([]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_gateway(
            &mut engine,
            GatewayReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
                host_probe: &host_probe,
            },
        ))
        .expect("reconcile missing gateway");

    assert_eq!(result.action(), GatewayReconcileAction::Created);
    assert_eq!(result.health(), ContainerHealth::Starting);
    assert_eq!(result.container().id().as_str(), "created-gateway");
    assert_eq!(engine.images, vec![request.image().to_owned()]);
    assert_eq!(engine.created, vec![request]);
    assert_eq!(engine.started.len(), 1);
    assert_eq!(host_probe.addresses.borrow().len(), 4);
}

#[test]
fn gateway_reconciliation_leaves_an_owned_healthy_gateway_unchanged() {
    let metadata = gateway_metadata("sha256:gateway-v1");
    let request = gateway_request(metadata.clone());
    let observed = ObservedContainer::new(
        crate::control_plane::engine::ContainerId::new("gateway-1"),
        metadata.labels(),
    );
    let mut engine = RecordingGatewayEngine {
        observed: vec![observed],
        state: ContainerState::Running,
        health: ContainerHealth::Healthy,
        ..RecordingGatewayEngine::default()
    };
    let host_probe = RecordingGatewayPortProbe::with_results([]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_gateway(
            &mut engine,
            GatewayReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
                host_probe: &host_probe,
            },
        ))
        .expect("reconcile healthy gateway");

    assert_eq!(result.action(), GatewayReconcileAction::Unchanged);
    assert_eq!(result.health(), ContainerHealth::Healthy);
    assert!(engine.created.is_empty());
    assert!(engine.started.is_empty());
    assert!(host_probe.addresses.borrow().is_empty());
}

#[test]
fn gateway_reconciliation_starts_a_stopped_owned_gateway() {
    let metadata = gateway_metadata("sha256:gateway-v1");
    let request = gateway_request(metadata.clone());
    let observed = ObservedContainer::new(
        crate::control_plane::engine::ContainerId::new("gateway-1"),
        metadata.labels(),
    );
    let mut engine = RecordingGatewayEngine {
        observed: vec![observed],
        state: ContainerState::Stopped,
        health: ContainerHealth::Starting,
        ..RecordingGatewayEngine::default()
    };
    let host_probe = RecordingGatewayPortProbe::with_results([]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_gateway(
            &mut engine,
            GatewayReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
                host_probe: &host_probe,
            },
        ))
        .expect("reconcile stopped gateway");

    assert_eq!(result.action(), GatewayReconcileAction::Started);
    assert_eq!(engine.started.len(), 1);
    assert_eq!(host_probe.addresses.borrow().len(), 4);
}

#[test]
fn gateway_reconciliation_replaces_owned_disposable_revision_drift() {
    let old_metadata = gateway_metadata("sha256:gateway-v1");
    let request = gateway_request(gateway_metadata("sha256:gateway-v2"));
    let observed = ObservedContainer::new(
        crate::control_plane::engine::ContainerId::new("gateway-1"),
        old_metadata.labels(),
    );
    let mut engine = RecordingGatewayEngine {
        observed: vec![observed],
        published: vec![
            PublishedPortBinding::new(
                "gateway-1",
                "stackctl-gateway",
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                80,
            )
            .expect("owned HTTP binding"),
            PublishedPortBinding::new(
                "gateway-1",
                "stackctl-gateway",
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                443,
            )
            .expect("owned HTTPS binding"),
        ],
        state: ContainerState::Running,
        health: ContainerHealth::Starting,
        ..RecordingGatewayEngine::default()
    };
    let host_probe = RecordingGatewayPortProbe::with_results([]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_gateway(
            &mut engine,
            GatewayReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
                host_probe: &host_probe,
            },
        ))
        .expect("replace drifted gateway");

    assert_eq!(result.action(), GatewayReconcileAction::Replaced);
    assert_eq!(engine.stopped.len(), 1);
    assert_eq!(engine.removed.len(), 1);
    assert_eq!(engine.created, vec![request]);
    assert_eq!(engine.started.len(), 1);
}

#[test]
fn gateway_reconciliation_restarts_an_unhealthy_owned_gateway() {
    let metadata = gateway_metadata("sha256:gateway-v1");
    let request = gateway_request(metadata.clone());
    let observed = ObservedContainer::new(
        crate::control_plane::engine::ContainerId::new("gateway-1"),
        metadata.labels(),
    );
    let mut engine = RecordingGatewayEngine {
        observed: vec![observed],
        published: vec![
            PublishedPortBinding::new(
                "gateway-1",
                "stackctl-gateway",
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                80,
            )
            .expect("owned HTTP binding"),
        ],
        state: ContainerState::Running,
        health: ContainerHealth::Unhealthy { failing_streak: 3 },
        ..RecordingGatewayEngine::default()
    };
    let host_probe = RecordingGatewayPortProbe::with_results([]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_gateway(
            &mut engine,
            GatewayReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
                host_probe: &host_probe,
            },
        ))
        .expect("restart unhealthy gateway");

    assert_eq!(result.action(), GatewayReconcileAction::Restarted);
    assert_eq!(engine.stopped.len(), 1);
    assert_eq!(engine.started.len(), 1);
    assert!(engine.removed.is_empty());
}

#[test]
fn gateway_replacement_fails_before_mutation_when_a_foreign_binding_conflicts() {
    let old_metadata = gateway_metadata("sha256:gateway-v1");
    let request = gateway_request(gateway_metadata("sha256:gateway-v2"));
    let observed = ObservedContainer::new(
        crate::control_plane::engine::ContainerId::new("gateway-1"),
        old_metadata.labels(),
    );
    let mut engine = RecordingGatewayEngine {
        observed: vec![observed],
        published: vec![
            PublishedPortBinding::new(
                "gateway-1",
                "stackctl-gateway",
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                443,
            )
            .expect("owned HTTPS binding"),
            PublishedPortBinding::new(
                "foreign-1",
                "foreign-proxy",
                IpAddr::V6(Ipv6Addr::LOCALHOST),
                443,
            )
            .expect("foreign HTTPS binding"),
        ],
        state: ContainerState::Running,
        ..RecordingGatewayEngine::default()
    };
    let host_probe = RecordingGatewayPortProbe::with_results([]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(reconcile_gateway(
            &mut engine,
            GatewayReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
                host_probe: &host_probe,
            },
        ))
        .expect_err("foreign binding must block replacement");

    assert!(
        error
            .to_string()
            .contains("[::1]:443 is occupied by Engine container 'foreign-proxy'")
    );
    assert!(engine.stopped.is_empty());
    assert!(engine.removed.is_empty());
    assert!(engine.created.is_empty());
}

#[test]
fn gateway_reconciliation_rejects_multiple_owned_gateway_containers() {
    let metadata = gateway_metadata("sha256:gateway-v1");
    let request = gateway_request(metadata.clone());
    let mut engine = RecordingGatewayEngine {
        observed: vec![
            ObservedContainer::new(
                crate::control_plane::engine::ContainerId::new("gateway-1"),
                metadata.labels(),
            ),
            ObservedContainer::new(
                crate::control_plane::engine::ContainerId::new("gateway-2"),
                metadata.labels(),
            ),
        ],
        ..RecordingGatewayEngine::default()
    };
    let host_probe = RecordingGatewayPortProbe::with_results([]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(reconcile_gateway(
            &mut engine,
            GatewayReconcileOptions {
                request: &request,
                installation_id: "install-1",
                schema_version: 8,
                host_probe: &host_probe,
            },
        ))
        .expect_err("ambiguous gateways");

    assert_eq!(
        error.to_string(),
        "cannot reconcile gateway because installation 'install-1' owns 2 gateway containers"
    );
    assert!(engine.created.is_empty());
    assert!(host_probe.addresses.borrow().is_empty());
}

#[test]
fn gateway_readiness_waits_until_the_owned_container_is_healthy() {
    let gateway = owned_gateway("gateway-1", gateway_metadata("sha256:gateway-v1"));
    let observer = SequencedHealthObserver::new([
        ContainerHealth::Starting,
        ContainerHealth::RunningUnverified,
        ContainerHealth::Healthy,
    ]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("test runtime");

    let health = runtime
        .block_on(wait_for_gateway_ready(
            &observer,
            GatewayReadinessOptions::new(
                &gateway,
                Duration::from_millis(100),
                Duration::from_millis(1),
            )
            .expect("readiness options"),
        ))
        .expect("healthy gateway");

    assert_eq!(health, ContainerHealth::Healthy);
    assert_eq!(observer.observations(), 3);
}

#[test]
fn gateway_readiness_times_out_with_the_last_observed_health() {
    let gateway = owned_gateway("gateway-1", gateway_metadata("sha256:gateway-v1"));
    let observer = SequencedHealthObserver::new([ContainerHealth::Starting]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(wait_for_gateway_ready(
            &observer,
            GatewayReadinessOptions::new(
                &gateway,
                Duration::from_millis(5),
                Duration::from_millis(1),
            )
            .expect("readiness options"),
        ))
        .expect_err("starting gateway must time out");

    assert!(
        error
            .to_string()
            .contains("did not become healthy within 5ms; last observed health: Starting")
    );
}

#[test]
fn gateway_plane_reconciles_container_readiness_then_route_revision() {
    let request = gateway_request(gateway_metadata("sha256:gateway-v1"));
    let snapshot = GatewaySnapshot::new(Vec::new()).expect("snapshot");
    let mut engine = RecordingGatewayEngine {
        health_sequence: Mutex::new(VecDeque::from([
            ContainerHealth::Starting,
            ContainerHealth::Healthy,
        ])),
        ..RecordingGatewayEngine::default()
    };
    let mut provider = RecordingGatewayProvider::default();
    let host_probe = RecordingGatewayPortProbe::with_results([]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("test runtime");

    let result = runtime
        .block_on(reconcile_gateway_plane(
            &mut engine,
            &mut provider,
            GatewayPlaneOptions::new(
                GatewayReconcileOptions {
                    request: &request,
                    installation_id: "install-1",
                    schema_version: 8,
                    host_probe: &host_probe,
                },
                &snapshot,
                Duration::from_millis(100),
                Duration::from_millis(1),
            )
            .expect("gateway plane options"),
        ))
        .expect("reconcile gateway plane");

    assert_eq!(result.gateway_action(), GatewayReconcileAction::Created);
    assert_eq!(result.health(), ContainerHealth::Healthy);
    assert_eq!(
        result.configuration_action(),
        GatewayConfigurationAction::Applied
    );
    assert_eq!(provider.applied, vec![snapshot]);
}

#[test]
fn gateway_plane_never_applies_routes_when_readiness_fails() {
    let request = gateway_request(gateway_metadata("sha256:gateway-v1"));
    let snapshot = GatewaySnapshot::new(Vec::new()).expect("snapshot");
    let mut engine = RecordingGatewayEngine {
        health_sequence: Mutex::new(VecDeque::from([
            ContainerHealth::Starting,
            ContainerHealth::Unhealthy { failing_streak: 1 },
        ])),
        ..RecordingGatewayEngine::default()
    };
    let mut provider = RecordingGatewayProvider::default();
    let host_probe = RecordingGatewayPortProbe::with_results([]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("test runtime");

    runtime
        .block_on(reconcile_gateway_plane(
            &mut engine,
            &mut provider,
            GatewayPlaneOptions::new(
                GatewayReconcileOptions {
                    request: &request,
                    installation_id: "install-1",
                    schema_version: 8,
                    host_probe: &host_probe,
                },
                &snapshot,
                Duration::from_millis(100),
                Duration::from_millis(1),
            )
            .expect("gateway plane options"),
        ))
        .expect_err("unhealthy gateway");

    assert!(provider.applied.is_empty());
}

#[test]
fn gateway_configuration_is_an_object_safe_atomic_strategy() {
    let snapshot = GatewaySnapshot::new(vec![
        GatewayRoute::new("shop-app.stackctl.localhost", "http://shop-app:8080")
            .expect("app route"),
    ])
    .expect("complete snapshot");
    let mut provider = RecordingGatewayProvider::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    runtime
        .block_on(apply_through_strategy(&mut provider, &snapshot))
        .expect("atomic apply");

    assert_eq!(provider.applied, vec![snapshot]);
}

#[test]
fn gateway_configuration_reconciliation_skips_the_active_revision() {
    let snapshot = GatewaySnapshot::new(Vec::new()).expect("snapshot");
    let mut provider = RecordingGatewayProvider {
        active_revision: Some(snapshot.revision().to_owned()),
        ..RecordingGatewayProvider::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let action = runtime
        .block_on(reconcile_gateway_configuration(&mut provider, &snapshot))
        .expect("reconcile active snapshot");

    assert_eq!(action, GatewayConfigurationAction::Unchanged);
    assert!(provider.applied.is_empty());
}

#[test]
fn gateway_configuration_reconciliation_applies_and_verifies_the_desired_revision() {
    let snapshot = GatewaySnapshot::new(Vec::new()).expect("snapshot");
    let mut provider = RecordingGatewayProvider::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let action = runtime
        .block_on(reconcile_gateway_configuration(&mut provider, &snapshot))
        .expect("apply desired snapshot");

    assert_eq!(action, GatewayConfigurationAction::Applied);
    assert_eq!(provider.applied, vec![snapshot.clone()]);
    assert_eq!(
        provider.active_revision.as_deref(),
        Some(snapshot.revision())
    );
}

#[test]
fn gateway_configuration_reconciliation_fails_when_revision_cannot_be_verified() {
    let snapshot = GatewaySnapshot::new(Vec::new()).expect("snapshot");
    let mut provider = RecordingGatewayProvider {
        reported_revision_after_apply: Some("sha256:stale".to_owned()),
        ..RecordingGatewayProvider::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(reconcile_gateway_configuration(&mut provider, &snapshot))
        .expect_err("stale active revision");

    assert_eq!(
        error.to_string(),
        format!(
            "gateway applied revision '{}' but reported active revision 'sha256:stale'",
            snapshot.revision()
        )
    );
}

#[test]
fn caddy_document_uses_stackctl_tls_plain_upstreams_and_private_admin_endpoint() {
    let snapshot = GatewaySnapshot::new(vec![
        GatewayRoute::new("shop-app.stackctl.localhost", "http://shop-app:8080")
            .expect("app route"),
    ])
    .expect("complete snapshot");

    let document = render_caddy_document(
        &snapshot,
        Path::new("/etc/stackctl/tls/leaf.pem"),
        Path::new("/etc/stackctl/tls/leaf-key.pem"),
        "localhost:2019",
    )
    .expect("valid Caddy document");
    let json: Value = serde_json::from_slice(document.bytes()).expect("Caddy JSON");

    assert_eq!(
        json.pointer("/admin/listen").and_then(Value::as_str),
        Some("localhost:2019")
    );
    assert_eq!(
        json.pointer("/admin/config/persist")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        json.pointer("/apps/tls/certificates/load_files/0/certificate")
            .and_then(Value::as_str),
        Some("/etc/stackctl/tls/leaf.pem")
    );
    assert_eq!(
        json.pointer("/apps/http/servers/stackctl_https/routes/0/handle/0/upstreams/0/dial")
            .and_then(Value::as_str),
        Some("shop-app:8080")
    );
    assert_eq!(
        json.pointer("/apps/http/servers/stackctl_https/routes/0/handle/0/handler")
            .and_then(Value::as_str),
        Some("reverse_proxy")
    );
    assert_eq!(
        json.pointer("/apps/http/servers/stackctl_http/routes/0/handle/0/handler")
            .and_then(Value::as_str),
        Some("static_response")
    );
    assert_eq!(
        json.pointer("/apps/http/servers/stackctl_http/routes/0/handle/0/status_code")
            .and_then(Value::as_u64),
        Some(308)
    );
    assert_eq!(
        json.pointer("/apps/http/servers/stackctl_http/routes/0/handle/0/headers/Location/0")
            .and_then(Value::as_str),
        Some("https://{http.request.host}{http.request.uri}")
    );
    assert!(
        json.pointer("/apps/http/servers/stackctl_http/routes/0/handle/0/upstreams")
            .is_none()
    );
    assert_eq!(
        json.pointer("/apps/http/servers/stackctl_https/tls_connection_policies/0"),
        Some(&serde_json::json!({}))
    );
    assert_eq!(
        json.pointer("/apps/http/grace_period")
            .and_then(Value::as_str),
        Some("30s")
    );
    assert!(json.pointer("/grace_period").is_none());
    assert!(json.pointer("/apps/pki").is_none());
    assert_eq!(document.revision(), snapshot.revision());
}

#[cfg(unix)]
#[test]
fn caddy_bootstrap_is_atomic_private_and_idempotent() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-gateway-bootstrap-{}-{}",
        std::process::id(),
        unique_test_value()
    ));
    let config_path = root.join("config/config.json");
    let snapshot = GatewaySnapshot::new(Vec::new()).unwrap();
    let document = render_caddy_document(
        &snapshot,
        Path::new("/etc/stackctl/tls/leaf.pem"),
        Path::new("/etc/stackctl/tls/leaf-key.pem"),
        "localhost:2019",
    )
    .unwrap();

    let stored = store_caddy_bootstrap(&document, &config_path).expect("store bootstrap");
    store_caddy_bootstrap(&document, &config_path).expect("repeat bootstrap");

    assert_eq!(stored.config_path(), config_path);
    assert_eq!(std::fs::read(&config_path).unwrap(), document.bytes());

    #[cfg(unix)]
    {
        assert_eq!(
            std::fs::metadata(&config_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    std::fs::remove_dir_all(root).expect("remove bootstrap directory");
}

#[cfg(unix)]
#[test]
fn caddy_bootstrap_refuses_a_symbolic_link_without_touching_its_target() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "stackctl-gateway-bootstrap-symlink-{}-{}",
        std::process::id(),
        unique_test_value()
    ));
    let config_directory = root.join("config");
    std::fs::create_dir_all(&config_directory).expect("create config directory");
    let config_path = config_directory.join("config.json");
    let snapshot = GatewaySnapshot::new(Vec::new()).expect("empty gateway snapshot");
    let document = render_caddy_document(
        &snapshot,
        Path::new("/etc/stackctl/tls/leaf.pem"),
        Path::new("/etc/stackctl/tls/leaf-key.pem"),
        "localhost:2019",
    )
    .expect("gateway document");
    let victim = root.join("victim.json");
    std::fs::write(&victim, document.bytes()).expect("write victim");
    std::fs::set_permissions(&victim, std::fs::Permissions::from_mode(0o640))
        .expect("set victim permissions");
    symlink(&victim, &config_path).expect("create config symlink");

    let error = store_caddy_bootstrap(&document, &config_path)
        .expect_err("gateway bootstrap symlink must fail closed");

    assert!(error.to_string().contains("symbolic link"));
    assert_eq!(
        std::fs::read(&victim).expect("read victim"),
        document.bytes()
    );
    assert_eq!(
        std::fs::metadata(&victim)
            .expect("victim metadata")
            .permissions()
            .mode()
            & 0o777,
        0o640
    );

    std::fs::remove_dir_all(root).expect("remove bootstrap directory");
}

#[test]
fn caddy_provider_advances_revision_only_after_atomic_load_succeeds() {
    let snapshot = GatewaySnapshot::new(vec![
        GatewayRoute::new("shop-app.stackctl.localhost", "http://shop-app:8080")
            .expect("app route"),
    ])
    .expect("complete snapshot");
    let loader = RecordingDocumentLoader::default();
    let mut provider = CaddyGatewayProvider::new(
        loader,
        "/etc/stackctl/tls/leaf.pem",
        "/etc/stackctl/tls/leaf-key.pem",
        "localhost:2019",
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    runtime
        .block_on(provider.apply_snapshot(&snapshot))
        .expect("atomic load");

    assert_eq!(
        runtime.block_on(provider.active_revision()).unwrap(),
        Some(snapshot.revision().to_owned())
    );
    assert_eq!(provider.loader().documents.len(), 1);
}

#[test]
fn caddy_provider_keeps_previous_revision_when_load_fails() {
    let snapshot = GatewaySnapshot::new(Vec::new()).unwrap();
    let loader = RecordingDocumentLoader {
        failure: Some("Caddy rejected configuration".to_owned()),
        ..RecordingDocumentLoader::default()
    };
    let mut provider = CaddyGatewayProvider::new(
        loader,
        "/etc/stackctl/tls/leaf.pem",
        "/etc/stackctl/tls/leaf-key.pem",
        "localhost:2019",
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(provider.apply_snapshot(&snapshot))
        .expect_err("load failure");

    assert_eq!(error.to_string(), "Caddy rejected configuration");
    assert_eq!(runtime.block_on(provider.active_revision()).unwrap(), None);
}

fn unique_test_value() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos()
}

fn apply_through_strategy<'operation>(
    strategy: &'operation mut dyn GatewayConfiguration,
    snapshot: &'operation GatewaySnapshot,
) -> GatewayFuture<'operation, ()> {
    strategy.apply_snapshot(snapshot)
}

#[derive(Default)]
struct RecordingGatewayProvider {
    applied: Vec<GatewaySnapshot>,
    active_revision: Option<String>,
    reported_revision_after_apply: Option<String>,
}

#[derive(Default)]
struct RecordingDocumentLoader {
    documents: Vec<Vec<u8>>,
    failure: Option<String>,
}

impl GatewayDocumentLoader for RecordingDocumentLoader {
    fn load_document<'operation>(
        &'operation mut self,
        document: &'operation [u8],
    ) -> GatewayFuture<'operation, ()> {
        Box::pin(async move {
            if let Some(failure) = &self.failure {
                return Err(GatewayError::Provider {
                    detail: failure.clone(),
                });
            }

            self.documents.push(document.to_vec());
            Ok(())
        })
    }
}

impl GatewayConfiguration for RecordingGatewayProvider {
    fn apply_snapshot<'operation>(
        &'operation mut self,
        snapshot: &'operation GatewaySnapshot,
    ) -> GatewayFuture<'operation, ()> {
        Box::pin(async move {
            self.applied.push(snapshot.clone());
            self.active_revision = Some(
                self.reported_revision_after_apply
                    .clone()
                    .unwrap_or_else(|| snapshot.revision().to_owned()),
            );

            Ok(())
        })
    }

    fn active_revision(&self) -> GatewayFuture<'_, Option<String>> {
        let active_revision = self.active_revision.clone();

        Box::pin(async move { Ok(active_revision) })
    }
}

struct RecordingLocalhostResolver {
    hosts: RefCell<Vec<String>>,
    addresses: Vec<IpAddr>,
}

impl RecordingLocalhostResolver {
    fn returning(addresses: Vec<IpAddr>) -> Self {
        Self {
            hosts: RefCell::default(),
            addresses,
        }
    }
}

impl LocalhostResolver for RecordingLocalhostResolver {
    fn resolve(&self, host: &str) -> Result<Vec<IpAddr>, GatewayError> {
        self.hosts.borrow_mut().push(host.to_owned());

        Ok(self.addresses.clone())
    }
}

struct RecordingGatewayPortProbe {
    addresses: RefCell<Vec<SocketAddr>>,
    results: BTreeMap<SocketAddr, GatewayPortAvailability>,
}

impl RecordingGatewayPortProbe {
    fn with_results(
        results: impl IntoIterator<Item = (SocketAddr, GatewayPortAvailability)>,
    ) -> Self {
        Self {
            addresses: RefCell::default(),
            results: results.into_iter().collect(),
        }
    }
}

impl GatewayPortProbe for RecordingGatewayPortProbe {
    fn probe(&self, address: SocketAddr) -> Result<GatewayPortAvailability, GatewayError> {
        self.addresses.borrow_mut().push(address);

        Ok(self
            .results
            .get(&address)
            .cloned()
            .unwrap_or(GatewayPortAvailability::Available))
    }
}

struct RecordingPublishedPortDiscovery {
    bindings: Vec<PublishedPortBinding>,
}

impl PublishedPortDiscovery for RecordingPublishedPortDiscovery {
    fn discover_published_tcp_ports(&self) -> EngineFuture<'_, Vec<PublishedPortBinding>> {
        Box::pin(async { Ok(self.bindings.clone()) })
    }
}

fn gateway_metadata(desired_revision: &str) -> ManagedResourceMetadata {
    ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::Gateway,
        project_id: None,
        compatibility_fingerprint: "sha256:gateway-profile".to_owned(),
        schema_version: 8,
        desired_revision: desired_revision.to_owned(),
        retention: RetentionClass::Disposable,
    })
    .expect("gateway metadata")
}

fn gateway_request(metadata: ManagedResourceMetadata) -> ContainerCreateOptions {
    gateway_container_request(GatewayContainerRequestOptions::new(
        concat!(
            "ghcr.io/stackctl/gateway@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )
        .to_owned(),
        "stackctl".to_owned(),
        "501:20".to_owned(),
        std::path::PathBuf::from("/state/tls/wildcard.crt"),
        std::path::PathBuf::from("/state/tls/wildcard.key"),
        std::path::PathBuf::from("/state/gateway/config.json"),
        metadata,
    ))
    .expect("gateway request")
}

fn owned_gateway(id: &str, metadata: ManagedResourceMetadata) -> OwnedContainer {
    reconstruct_owned_container(
        &ObservedContainer::new(
            crate::control_plane::engine::ContainerId::new(id),
            metadata.labels(),
        ),
        metadata.installation_id(),
        metadata.schema_version(),
    )
    .expect("owned gateway")
}

struct SequencedHealthObserver {
    health: Mutex<VecDeque<ContainerHealth>>,
    last: Mutex<ContainerHealth>,
    observations: AtomicUsize,
}

impl SequencedHealthObserver {
    fn new(health: impl IntoIterator<Item = ContainerHealth>) -> Self {
        Self {
            health: Mutex::new(health.into_iter().collect()),
            last: Mutex::new(ContainerHealth::Starting),
            observations: AtomicUsize::new(0),
        }
    }

    fn observations(&self) -> usize {
        self.observations.load(Ordering::SeqCst)
    }
}

impl HealthObserver for SequencedHealthObserver {
    fn observe_health<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ContainerHealth> {
        Box::pin(async move {
            self.observations.fetch_add(1, Ordering::SeqCst);
            if let Some(health) = self.health.lock().expect("health lock").pop_front() {
                *self.last.lock().expect("last health lock") = health;
            }

            Ok(*self.last.lock().expect("last health lock"))
        })
    }
}

struct RecordingGatewayEngine {
    images: Vec<String>,
    observed: Vec<ObservedContainer>,
    published: Vec<PublishedPortBinding>,
    state: ContainerState,
    health: ContainerHealth,
    health_sequence: Mutex<VecDeque<ContainerHealth>>,
    created: Vec<ContainerCreateOptions>,
    started: Vec<OwnedContainer>,
    stopped: Vec<OwnedContainer>,
    removed: Vec<OwnedContainer>,
}

impl Default for RecordingGatewayEngine {
    fn default() -> Self {
        Self {
            images: Vec::new(),
            observed: Vec::new(),
            published: Vec::new(),
            state: ContainerState::Missing,
            health: ContainerHealth::Starting,
            health_sequence: Mutex::new(VecDeque::new()),
            created: Vec::new(),
            started: Vec::new(),
            stopped: Vec::new(),
            removed: Vec::new(),
        }
    }
}

impl ImageResolver for RecordingGatewayEngine {
    fn ensure_image<'operation>(
        &'operation mut self,
        reference: &'operation ImmutableImageReference,
    ) -> EngineFuture<'operation, ImageId> {
        Box::pin(async move {
            self.images.push(reference.as_str().to_owned());
            ImageId::new(format!("sha256:{}", "a".repeat(64)))
        })
    }
}

impl ContainerDiscovery for RecordingGatewayEngine {
    fn discover_managed(&self) -> EngineFuture<'_, Vec<ObservedContainer>> {
        Box::pin(async { Ok(self.observed.clone()) })
    }
}

impl PublishedPortDiscovery for RecordingGatewayEngine {
    fn discover_published_tcp_ports(&self) -> EngineFuture<'_, Vec<PublishedPortBinding>> {
        Box::pin(async { Ok(self.published.clone()) })
    }
}

impl ContainerLifecycle for RecordingGatewayEngine {
    fn create<'operation>(
        &'operation mut self,
        options: &'operation ContainerCreateOptions,
    ) -> EngineFuture<'operation, OwnedContainer> {
        Box::pin(async move {
            self.created.push(options.clone());
            let observed = ObservedContainer::new(
                crate::control_plane::engine::ContainerId::new("created-gateway"),
                options.metadata().labels(),
            );

            reconstruct_owned_container(
                &observed,
                options.metadata().installation_id(),
                options.metadata().schema_version(),
            )
            .map_err(|ownership| EngineError::Backend {
                detail: format!("could not reconstruct recorded gateway: {ownership:?}"),
            })
        })
    }

    fn start<'operation>(
        &'operation mut self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.started.push(container.clone());
            Ok(())
        })
    }

    fn stop<'operation>(
        &'operation mut self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.stopped.push(container.clone());
            Ok(())
        })
    }

    fn remove<'operation>(
        &'operation mut self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.removed.push(container.clone());
            Ok(())
        })
    }

    fn inspect<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ContainerState> {
        Box::pin(async { Ok(self.state) })
    }
}

impl HealthObserver for RecordingGatewayEngine {
    fn observe_health<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ContainerHealth> {
        Box::pin(async {
            Ok(self
                .health_sequence
                .lock()
                .expect("health sequence lock")
                .pop_front()
                .unwrap_or(self.health))
        })
    }
}
