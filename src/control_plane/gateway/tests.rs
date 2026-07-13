use super::{
    CaddyGatewayProvider, EngineGatewayPortProbe, GatewayConfiguration, GatewayConfigurationAction,
    GatewayDocumentLoader, GatewayError, GatewayFuture, GatewayPortAvailability, GatewayPortProbe,
    GatewayReadinessOptions, GatewayReconcileAction, GatewayReconcileOptions, GatewayRoute,
    GatewaySnapshot, LocalhostResolver, SystemGatewayPortProbe, preflight_gateway_ports,
    reconcile_gateway, reconcile_gateway_configuration, render_caddy_document,
    store_caddy_bootstrap, verify_gateway_ports_available, verify_stackctl_localhost_resolution,
    wait_for_gateway_ready,
};
use crate::control_plane::engine::{
    ContainerCreateOptions, ContainerDiscovery, ContainerHealth, ContainerLifecycle,
    ContainerState, EngineError, EngineFuture, GatewayContainerRequestOptions, HealthObserver,
    ManagedResourceMetadata, ManagedResourceMetadataOptions, ObservedContainer, OwnedContainer,
    PublishedPortBinding, PublishedPortDiscovery, ResourceKind, RetentionClass,
    gateway_container_request, reconstruct_owned_container,
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
use super::CaddyUnixAdminClient;

#[cfg(unix)]
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[cfg(unix)]
use tokio::net::UnixListener;

#[test]
fn complete_route_snapshots_are_sorted_before_provider_application() {
    let snapshot = GatewaySnapshot::new(
        "sha256:routes-v1",
        vec![
            GatewayRoute::new("shop-mailpit.stackctl.localhost", "http://mailpit:8025")
                .expect("mail route"),
            GatewayRoute::new("shop-app.stackctl.localhost", "http://shop-app:8080")
                .expect("app route"),
        ],
    )
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
    assert_eq!(snapshot.revision(), "sha256:routes-v1");
}

#[test]
fn duplicate_domains_reject_the_entire_gateway_snapshot() {
    let error = GatewaySnapshot::new(
        "sha256:routes-v1",
        vec![
            GatewayRoute::new("shop-app.stackctl.localhost", "http://shop-app:8080")
                .expect("first route"),
            GatewayRoute::new("shop-app.stackctl.localhost", "http://other-app:8080")
                .expect("second route"),
        ],
    )
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
fn gateway_configuration_is_an_object_safe_atomic_strategy() {
    let snapshot = GatewaySnapshot::new(
        "sha256:routes-v1",
        vec![
            GatewayRoute::new("shop-app.stackctl.localhost", "http://shop-app:8080")
                .expect("app route"),
        ],
    )
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
    let snapshot = GatewaySnapshot::new("sha256:routes-v1", Vec::new()).expect("snapshot");
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
    let snapshot = GatewaySnapshot::new("sha256:routes-v1", Vec::new()).expect("snapshot");
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
    let snapshot = GatewaySnapshot::new("sha256:routes-v1", Vec::new()).expect("snapshot");
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
        "gateway applied revision 'sha256:routes-v1' but reported active revision 'sha256:stale'"
    );
}

#[test]
fn caddy_document_uses_stackctl_tls_plain_upstreams_and_private_admin_socket() {
    let snapshot = GatewaySnapshot::new(
        "sha256:routes-v1",
        vec![
            GatewayRoute::new("shop-app.stackctl.localhost", "http://shop-app:8080")
                .expect("app route"),
        ],
    )
    .expect("complete snapshot");

    let document = render_caddy_document(
        &snapshot,
        Path::new("/etc/stackctl/tls/leaf.pem"),
        Path::new("/etc/stackctl/tls/leaf-key.pem"),
        Path::new("/run/stackctl/admin.sock"),
    )
    .expect("valid Caddy document");
    let json: Value = serde_json::from_slice(document.bytes()).expect("Caddy JSON");

    assert_eq!(
        json.pointer("/admin/listen").and_then(Value::as_str),
        Some("unix//run/stackctl/admin.sock|0600")
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
    let runtime_directory = root.join("run");
    let snapshot = GatewaySnapshot::new("sha256:routes-v1", Vec::new()).unwrap();
    let document = render_caddy_document(
        &snapshot,
        Path::new("/etc/stackctl/tls/leaf.pem"),
        Path::new("/etc/stackctl/tls/leaf-key.pem"),
        Path::new("/run/stackctl/admin.sock"),
    )
    .unwrap();

    let stored = store_caddy_bootstrap(&document, &config_path, &runtime_directory)
        .expect("store bootstrap");
    store_caddy_bootstrap(&document, &config_path, &runtime_directory).expect("repeat bootstrap");

    assert_eq!(stored.config_path(), config_path);
    assert_eq!(stored.runtime_directory(), runtime_directory);
    assert_eq!(
        stored.admin_socket_path(),
        runtime_directory.join("admin.sock")
    );
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
        assert_eq!(
            std::fs::metadata(&runtime_directory)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }

    std::fs::remove_dir_all(root).expect("remove bootstrap directory");
}

#[test]
fn caddy_provider_advances_revision_only_after_atomic_load_succeeds() {
    let snapshot = GatewaySnapshot::new(
        "sha256:routes-v1",
        vec![
            GatewayRoute::new("shop-app.stackctl.localhost", "http://shop-app:8080")
                .expect("app route"),
        ],
    )
    .expect("complete snapshot");
    let loader = RecordingDocumentLoader::default();
    let mut provider = CaddyGatewayProvider::new(
        loader,
        "/etc/stackctl/tls/leaf.pem",
        "/etc/stackctl/tls/leaf-key.pem",
        "/run/stackctl/admin.sock",
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    runtime
        .block_on(provider.apply_snapshot(&snapshot))
        .expect("atomic load");

    assert_eq!(
        runtime.block_on(provider.active_revision()).unwrap(),
        Some("sha256:routes-v1".to_owned())
    );
    assert_eq!(provider.loader().documents.len(), 1);
}

#[test]
fn caddy_provider_keeps_previous_revision_when_load_fails() {
    let snapshot = GatewaySnapshot::new("sha256:routes-v1", Vec::new()).unwrap();
    let loader = RecordingDocumentLoader {
        failure: Some("Caddy rejected configuration".to_owned()),
        ..RecordingDocumentLoader::default()
    };
    let mut provider = CaddyGatewayProvider::new(
        loader,
        "/etc/stackctl/tls/leaf.pem",
        "/etc/stackctl/tls/leaf-key.pem",
        "/run/stackctl/admin.sock",
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

#[cfg(unix)]
#[test]
fn caddy_admin_client_loads_complete_json_over_private_unix_http() {
    let socket_path = std::env::temp_dir().join(format!(
        "stackctl-caddy-admin-{}-{}.sock",
        std::process::id(),
        unique_test_value()
    ));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    runtime.block_on(async {
        let listener = UnixListener::bind(&socket_path).expect("bind admin socket");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept request");
            let mut request = vec![0; 4096];
            let length = stream.read(&mut request).await.expect("read request");
            request.truncate(length);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await
                .expect("write response");
            request
        });
        let mut client = CaddyUnixAdminClient::new(&socket_path);

        client
            .load_document(br#"{"apps":{}}"#)
            .await
            .expect("atomic Caddy load");
        let request = server.await.expect("server task");
        let request = String::from_utf8(request).expect("HTTP request");

        assert!(request.starts_with("POST /load HTTP/1.1\r\n"));
        assert!(request.contains("Content-Type: application/json\r\n"));
        assert!(request.ends_with(r#"{"apps":{}}"#));
    });

    std::fs::remove_file(&socket_path).expect("remove admin socket");
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
        std::path::PathBuf::from("/state/tls"),
        std::path::PathBuf::from("/state/gateway/config.json"),
        std::path::PathBuf::from("/state/gateway/run"),
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
    observed: Vec<ObservedContainer>,
    published: Vec<PublishedPortBinding>,
    state: ContainerState,
    health: ContainerHealth,
    created: Vec<ContainerCreateOptions>,
    started: Vec<OwnedContainer>,
    stopped: Vec<OwnedContainer>,
    removed: Vec<OwnedContainer>,
}

impl Default for RecordingGatewayEngine {
    fn default() -> Self {
        Self {
            observed: Vec::new(),
            published: Vec::new(),
            state: ContainerState::Missing,
            health: ContainerHealth::Starting,
            created: Vec::new(),
            started: Vec::new(),
            stopped: Vec::new(),
            removed: Vec::new(),
        }
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
        Box::pin(async { Ok(self.health) })
    }
}
