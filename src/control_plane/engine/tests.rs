use super::{
    CommandExecutor, CommandRequest, ContainerCreateOptions, ContainerDiscovery, ContainerEvent,
    ContainerEventAction, ContainerEventCursor, ContainerEventSource, ContainerEventStream,
    ContainerHealth, ContainerHealthCheck, ContainerId, ContainerLifecycle, ContainerLogOptions,
    ContainerLogStream, ContainerLogTail, ContainerResourceMetrics, ContainerState, EngineFuture,
    GatewayContainerRequestOptions, HealthObserver, ImageBuildRequest, ImageBuilder, ImageId,
    ImageResolver, ImmutableImageReference, LogChunk, LogSource, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, NetworkCreateOptions, NetworkDiscovery, NetworkId,
    NetworkManager, ObservedContainer, ObservedNetwork, ObservedResourceOwnership, ObservedVolume,
    OwnedContainer, OwnedNetwork, OwnedVolume, PublishedPortBinding, PublishedPortDiscovery,
    ResourceKind, ResourceMetrics, RetentionClass, VolumeCreateOptions, VolumeDiscovery,
    VolumeManager, VolumeMount, classify_observed_resource, gateway_container_request,
    reconstruct_owned_container, reconstruct_owned_network, reconstruct_owned_volume,
};
use bollard::ClientVersion;
use bollard::container::LogOutput;
use bollard::models::{
    ContainerCpuStats, ContainerCpuUsage, ContainerMemoryStats, ContainerNetworkStats,
    ContainerPidsStats, ContainerState as EngineContainerState, ContainerStatsResponse,
    ContainerSummary, EventActor, EventMessage, EventMessageTypeEnum, Health, HealthStatusEnum,
    Network, PortSummary, PortSummaryTypeEnum, Volume,
};
use futures_util::StreamExt;
use std::collections::BTreeMap;
use std::future::pending;
use std::time::Duration;

use super::bollard_engine_adapter::{
    build_image_options, command_create_request, container_event, container_health,
    container_resource_metrics, create_request, image_pull_request, log_chunk, log_request,
    managed_container_events_request, managed_container_list_request, managed_network_list_request,
    managed_volume_list_request, network_create_request, observed_container, observed_network,
    observed_volume, published_port_bindings, published_port_list_request,
    validate_engine_api_version, verify_owned_container_labels, verify_owned_network_labels,
    verify_owned_volume_labels, volume_create_request,
};
use super::bounded_engine_operation::bounded_engine_operation;

#[test]
fn managed_metadata_generates_complete_reserved_ownership_labels() {
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::SharedService,
        project_id: Some("bill".to_owned()),
        compatibility_fingerprint: "sha256:abc123".to_owned(),
        schema_version: 8,
        desired_revision: "sha256:def456".to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("valid managed metadata");

    assert_eq!(
        metadata.labels(),
        BTreeMap::from([
            (
                "dev.stackctl.fingerprint".to_owned(),
                "sha256:abc123".to_owned()
            ),
            (
                "dev.stackctl.installation".to_owned(),
                "install-1".to_owned()
            ),
            ("dev.stackctl.kind".to_owned(), "shared_service".to_owned()),
            ("dev.stackctl.managed".to_owned(), "true".to_owned()),
            ("dev.stackctl.project".to_owned(), "bill".to_owned()),
            ("dev.stackctl.schema".to_owned(), "8".to_owned()),
            (
                "dev.stackctl.desired".to_owned(),
                "sha256:def456".to_owned()
            ),
            ("dev.stackctl.retention".to_owned(), "persistent".to_owned()),
        ])
    );
}

#[test]
fn container_lifecycle_is_an_object_safe_replaceable_strategy() {
    let metadata = project_metadata(ResourceKind::ProjectApplication);
    let options = ContainerCreateOptions::new(
        "stackctl-bill-app",
        concat!(
            "ghcr.io/stackctl/php@sha256:",
            "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
        ),
        metadata,
    )
    .expect("immutable container options");
    let mut backend = RecordingContainerBackend::default();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");
    let container = runtime
        .block_on(create_through_strategy(&mut backend, &options))
        .expect("create container");

    assert_eq!(container.id().as_str(), "container-1");
    assert_eq!(backend.created, vec![options]);
}

#[test]
fn container_mutation_rejects_missing_or_changed_ownership_labels() {
    let container = OwnedContainer::new(
        ContainerId::new("container-1"),
        project_metadata(ResourceKind::ProjectApplication),
    );

    let error = verify_owned_container_labels(&container, &std::collections::HashMap::new())
        .expect_err("unlabelled container");

    assert_eq!(
        error.to_string(),
        "refusing to mutate container 'container-1' because its Engine ownership labels no longer match"
    );
}

#[test]
fn mutable_image_tags_are_rejected_before_an_engine_request() {
    let metadata = global_metadata(ResourceKind::Gateway);

    let error = ContainerCreateOptions::new("stackctl-gateway", "caddy:latest", metadata)
        .expect_err("mutable image reference");

    assert_eq!(
        error.to_string(),
        "managed image 'caddy:latest' must use an immutable sha256 digest"
    );
}

#[test]
fn image_resolution_accepts_only_immutable_digest_references() {
    let immutable = ImmutableImageReference::new(concat!(
        "ghcr.io/stackctl/php@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    ))
    .expect("immutable image");
    let error =
        ImmutableImageReference::new("ghcr.io/stackctl/php:8.4").expect_err("mutable image tag");
    let mut resolver = RecordingImageResolver::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");
    let strategy: &mut dyn ImageResolver = &mut resolver;

    let image = runtime
        .block_on(strategy.ensure_image(&immutable))
        .expect("resolve image");

    assert_eq!(image.as_str(), "sha256:image-config");
    assert_eq!(resolver.resolved, vec![immutable]);
    assert_eq!(
        error.to_string(),
        "managed image 'ghcr.io/stackctl/php:8.4' must use an immutable sha256 digest"
    );
}

#[test]
fn image_pull_requests_preserve_the_complete_digest_reference() {
    let reference = ImmutableImageReference::new(concat!(
        "ghcr.io/stackctl/php@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    ))
    .expect("immutable image");

    let request = image_pull_request(&reference);

    assert_eq!(request.from_image.as_deref(), Some(reference.as_str()));
    assert_eq!(request.tag, None);
}

#[test]
fn managed_container_events_are_scoped_and_resume_without_replaying_the_cursor() {
    let processed = ContainerEvent::new(
        ContainerId::new("container-1"),
        ContainerEventAction::Started,
        2_500_000_000,
    );
    let cursor = ContainerEventCursor::beginning().advance(&processed);
    let request = managed_container_events_request("installation-1", &cursor)
        .expect("valid event subscription");

    assert_eq!(request.since.as_deref(), Some("2"));
    assert_eq!(
        request.filters,
        Some(std::collections::HashMap::from([
            ("type".to_owned(), vec!["container".to_owned()]),
            (
                "label".to_owned(),
                vec![
                    "dev.stackctl.managed=true".to_owned(),
                    "dev.stackctl.installation=installation-1".to_owned(),
                ],
            ),
        ]))
    );

    let replayed = EventMessage {
        typ: Some(EventMessageTypeEnum::CONTAINER),
        action: Some("start".to_owned()),
        actor: Some(EventActor {
            id: Some("container-1".to_owned()),
            ..EventActor::default()
        }),
        time_nano: Some(2_500_000_000),
        ..EventMessage::default()
    };
    let new_health_event = EventMessage {
        action: Some("health_status: unhealthy".to_owned()),
        time_nano: Some(2_500_000_001),
        ..replayed.clone()
    };
    let simultaneous_distinct_event = EventMessage {
        action: Some("die".to_owned()),
        ..replayed.clone()
    };

    assert_eq!(
        container_event(replayed, &cursor).expect("valid event"),
        None
    );
    assert!(
        container_event(simultaneous_distinct_event, &cursor)
            .expect("valid event")
            .is_some()
    );
    let event = container_event(new_health_event, &cursor)
        .expect("valid event")
        .expect("new event");
    assert_eq!(event.container_id().as_str(), "container-1");
    assert_eq!(event.action(), &ContainerEventAction::HealthUnhealthy);
    assert_eq!(event.occurred_at_nanoseconds(), 2_500_000_001);
    assert_eq!(cursor.advance(&event).nanoseconds(), 2_500_000_001);
}

#[test]
fn managed_container_event_subscriptions_require_an_installation_identity() {
    let error = managed_container_events_request("", &ContainerEventCursor::beginning())
        .expect_err("empty installation identity");

    assert_eq!(
        error.to_string(),
        "managed event installation ID must not be empty"
    );
}

#[test]
fn container_event_source_is_streaming_and_object_safe() {
    let source = RecordingContainerEventSource;
    let strategy: &dyn ContainerEventSource = &source;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let event = runtime
        .block_on(async {
            strategy
                .stream_managed("installation-1", ContainerEventCursor::beginning())
                .next()
                .await
        })
        .expect("event item")
        .expect("valid event");

    assert_eq!(event.action(), &ContainerEventAction::Started);
}

#[test]
fn log_stream_options_are_typed_and_preserve_non_utf8_bytes() {
    let options =
        ContainerLogOptions::new(true, ContainerLogTail::last(25).expect("positive log tail"));
    let request = log_request(&options);
    let chunk = log_chunk(LogOutput::StdErr {
        message: vec![0xff, b'\n'].into(),
    });

    assert!(request.follow);
    assert!(request.stdout);
    assert!(request.stderr);
    assert!(!request.timestamps);
    assert_eq!(request.tail, "25");
    assert!(chunk.is_stderr());
    assert_eq!(chunk.bytes(), &[0xff, b'\n']);
    assert_eq!(
        ContainerLogTail::last(0)
            .expect_err("zero log tail")
            .to_string(),
        "container log tail must be greater than zero"
    );
}

#[test]
fn log_source_requires_an_owned_container_and_is_object_safe() {
    let source = RecordingLogSource;
    let strategy: &dyn LogSource = &source;
    let container = OwnedContainer::new(
        ContainerId::new("container-1"),
        global_metadata(ResourceKind::ProjectApplication),
    );
    let options = ContainerLogOptions::new(false, ContainerLogTail::all());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let chunk = runtime
        .block_on(async {
            let mut stream = strategy.logs(&container, &options).await?;
            stream.next().await.expect("log item")
        })
        .expect("valid log chunk");

    assert_eq!(chunk.bytes(), b"ready\n");
}

#[test]
fn command_requests_map_to_non_privileged_structured_engine_exec() {
    let request = CommandRequest::new(
        vec!["php".to_owned(), "artisan".to_owned(), "migrate".to_owned()],
        BTreeMap::from([
            ("APP_ENV".to_owned(), "local".to_owned()),
            ("DB_PASSWORD".to_owned(), "secret".to_owned()),
        ]),
        Some("/workspace".to_owned()),
    )
    .expect("valid command request");

    let engine_request = command_create_request(&request);

    assert_eq!(
        engine_request.cmd,
        Some(vec![
            "php".to_owned(),
            "artisan".to_owned(),
            "migrate".to_owned(),
        ])
    );
    assert_eq!(
        engine_request.env,
        Some(vec![
            "APP_ENV=local".to_owned(),
            "DB_PASSWORD=secret".to_owned(),
        ])
    );
    assert_eq!(engine_request.working_dir, Some("/workspace".to_owned()));
    assert_eq!(engine_request.attach_stdin, Some(true));
    assert_eq!(engine_request.attach_stdout, Some(true));
    assert_eq!(engine_request.attach_stderr, Some(true));
    assert_eq!(engine_request.tty, Some(false));
    assert_eq!(engine_request.privileged, Some(false));
    assert!(!format!("{request:?}").contains("secret"));
}

#[test]
fn managed_container_environment_maps_to_engine_without_debug_leaks() {
    let options = ContainerCreateOptions::new(
        "stackctl-shared-postgres",
        concat!(
            "postgres@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ),
        global_metadata(ResourceKind::SharedService),
    )
    .expect("container options")
    .with_environment(BTreeMap::from([
        ("POSTGRES_DB".to_owned(), "postgres".to_owned()),
        ("POSTGRES_PASSWORD".to_owned(), "root-secret".to_owned()),
    ]))
    .expect("container environment");

    let (_, request) = create_request(&options);

    assert_eq!(
        request.env,
        Some(vec![
            "POSTGRES_DB=postgres".to_owned(),
            "POSTGRES_PASSWORD=root-secret".to_owned(),
        ])
    );
    assert!(!format!("{options:?}").contains("root-secret"));
    assert!(format!("{options:?}").contains("POSTGRES_PASSWORD"));
}

#[test]
fn managed_named_volumes_remain_distinct_from_host_bind_mounts() {
    let options = ContainerCreateOptions::new(
        "stackctl-shared-postgres",
        concat!(
            "postgres@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ),
        global_metadata(ResourceKind::SharedService),
    )
    .expect("container options")
    .with_volume_mount(
        VolumeMount::read_write("stackctl-postgres-data", "/var/lib/postgresql/data")
            .expect("named volume mount"),
    );

    let (_, request) = create_request(&options);
    let mounts = request
        .host_config
        .expect("host config")
        .mounts
        .expect("mounts");

    assert_eq!(mounts.len(), 1);
    assert_eq!(mounts[0].typ, Some(bollard::models::MountType::VOLUME));
    assert_eq!(mounts[0].source.as_deref(), Some("stackctl-postgres-data"));
    assert_eq!(
        mounts[0].target.as_deref(),
        Some("/var/lib/postgresql/data")
    );
    assert_eq!(mounts[0].read_only, Some(false));
}

#[test]
fn managed_container_platform_is_explicitly_forwarded_to_the_engine() {
    let options = ContainerCreateOptions::new(
        "stackctl-shared-postgres",
        concat!(
            "postgres@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ),
        global_metadata(ResourceKind::SharedService),
    )
    .expect("container options")
    .with_platform("linux/arm64")
    .expect("Linux platform");

    let (query, _) = create_request(&options);

    assert_eq!(query.platform, "linux/arm64");
}

#[test]
fn command_requests_reject_ambiguous_or_unsafe_values_before_engine_access() {
    let empty_command =
        CommandRequest::new(Vec::new(), BTreeMap::new(), None).expect_err("empty command");
    let relative_directory = CommandRequest::new(
        vec!["php".to_owned()],
        BTreeMap::new(),
        Some("workspace".to_owned()),
    )
    .expect_err("relative container directory");
    let invalid_environment = CommandRequest::new(
        vec!["php".to_owned()],
        BTreeMap::from([("BAD=KEY".to_owned(), "value".to_owned())]),
        None,
    )
    .expect_err("invalid environment key");

    assert_eq!(
        empty_command.to_string(),
        "container command must not be empty"
    );
    assert_eq!(
        relative_directory.to_string(),
        "container command working directory 'workspace' must be absolute"
    );
    assert_eq!(
        invalid_environment.to_string(),
        "container command environment key 'BAD=KEY' is invalid"
    );
}

#[test]
fn command_executor_boundary_is_object_safe() {
    fn accepts_command_executor(_executor: &dyn CommandExecutor) {}

    let _ = accepts_command_executor;
}

#[test]
fn health_observations_distinguish_process_and_readiness_states() {
    let stopped = EngineContainerState {
        running: Some(false),
        ..EngineContainerState::default()
    };
    let unverified = EngineContainerState {
        running: Some(true),
        ..EngineContainerState::default()
    };
    let starting = EngineContainerState {
        running: Some(true),
        health: Some(Health {
            status: Some(HealthStatusEnum::STARTING),
            ..Health::default()
        }),
        ..EngineContainerState::default()
    };
    let healthy = EngineContainerState {
        health: Some(Health {
            status: Some(HealthStatusEnum::HEALTHY),
            ..Health::default()
        }),
        ..starting.clone()
    };
    let unhealthy = EngineContainerState {
        health: Some(Health {
            status: Some(HealthStatusEnum::UNHEALTHY),
            failing_streak: Some(3),
            ..Health::default()
        }),
        ..starting.clone()
    };

    assert_eq!(
        container_health(Some(&stopped)).unwrap(),
        ContainerHealth::Stopped
    );
    assert_eq!(
        container_health(Some(&unverified)).unwrap(),
        ContainerHealth::RunningUnverified
    );
    assert_eq!(
        container_health(Some(&starting)).unwrap(),
        ContainerHealth::Starting
    );
    assert_eq!(
        container_health(Some(&healthy)).unwrap(),
        ContainerHealth::Healthy
    );
    assert_eq!(
        container_health(Some(&unhealthy)).unwrap(),
        ContainerHealth::Unhealthy { failing_streak: 3 }
    );
}

#[test]
fn health_observer_boundary_is_object_safe() {
    fn accepts_health_observer(_observer: &dyn HealthObserver) {}

    let _ = accepts_health_observer;
}

#[test]
fn resource_metrics_normalize_idle_benchmark_inputs_without_floats() {
    let stats = ContainerStatsResponse {
        id: Some("container-1".to_owned()),
        cpu_stats: Some(ContainerCpuStats {
            cpu_usage: Some(ContainerCpuUsage {
                total_usage: Some(300),
                ..ContainerCpuUsage::default()
            }),
            system_cpu_usage: Some(1_000),
            online_cpus: Some(2),
            ..ContainerCpuStats::default()
        }),
        precpu_stats: Some(ContainerCpuStats {
            cpu_usage: Some(ContainerCpuUsage {
                total_usage: Some(100),
                ..ContainerCpuUsage::default()
            }),
            system_cpu_usage: Some(500),
            ..ContainerCpuStats::default()
        }),
        memory_stats: Some(ContainerMemoryStats {
            usage: Some(1_024),
            ..ContainerMemoryStats::default()
        }),
        pids_stats: Some(ContainerPidsStats {
            current: Some(3),
            ..ContainerPidsStats::default()
        }),
        networks: Some(std::collections::HashMap::from([
            (
                "eth0".to_owned(),
                ContainerNetworkStats {
                    rx_bytes: Some(10),
                    tx_bytes: Some(5),
                    ..ContainerNetworkStats::default()
                },
            ),
            (
                "eth1".to_owned(),
                ContainerNetworkStats {
                    rx_bytes: Some(20),
                    tx_bytes: Some(7),
                    ..ContainerNetworkStats::default()
                },
            ),
        ])),
        ..ContainerStatsResponse::default()
    };

    let metrics = container_resource_metrics(&stats, &ContainerId::new("container-1"))
        .expect("complete metrics");

    assert_eq!(
        metrics,
        ContainerResourceMetrics::new(Some(8_000), Some(1_024), Some(3), Some(30), Some(12))
    );
}

#[test]
fn resource_metrics_reject_stats_for_a_different_container() {
    let stats = ContainerStatsResponse {
        id: Some("foreign-container".to_owned()),
        ..ContainerStatsResponse::default()
    };

    let error = container_resource_metrics(&stats, &ContainerId::new("container-1"))
        .expect_err("foreign stats");

    assert_eq!(
        error.to_string(),
        "Engine stats belong to container 'foreign-container', expected 'container-1'"
    );
}

#[test]
fn resource_metrics_boundary_is_object_safe() {
    fn accepts_resource_metrics(_metrics: &dyn ResourceMetrics) {}

    let _ = accepts_resource_metrics;
}

#[test]
fn image_build_requests_are_content_addressed_labeled_and_offline() {
    let metadata = global_metadata(ResourceKind::Build);
    let request = ImageBuildRequest::new(
        vec![0x01, 0x02, 0x03],
        "Dockerfile".to_owned(),
        concat!(
            "FROM ghcr.io/stackctl/php@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
            "COPY . /workspace\n"
        )
        .to_owned(),
        "linux/arm64".to_owned(),
        metadata.clone(),
    )
    .expect("valid immutable build");

    let options = build_image_options(&request);
    let labels = options.labels.expect("build labels");

    assert_eq!(options.dockerfile, "Dockerfile");
    assert_eq!(options.t.as_deref(), Some(request.output_tag()));
    assert_eq!(options.networkmode.as_deref(), Some("none"));
    assert_eq!(options.pull.as_deref(), Some("false"));
    assert_eq!(options.remote, None);
    assert!(options.rm);
    assert!(options.forcerm);
    assert_eq!(options.platform, "linux/arm64");
    assert!(request.output_tag().starts_with("stackctl-build:"));
    assert_eq!(
        labels.get("dev.stackctl.build-input"),
        Some(&request.input_digest().to_owned())
    );
    for (key, value) in metadata.labels() {
        assert_eq!(labels.get(&key), Some(&value));
    }
    assert!(!format!("{request:?}").contains("01, 02, 03"));
}

#[test]
fn image_build_requests_reject_mutable_bases_and_remote_additions() {
    let mutable_base = ImageBuildRequest::new(
        vec![1],
        "Dockerfile".to_owned(),
        "FROM php:8.4\n".to_owned(),
        "linux/amd64".to_owned(),
        global_metadata(ResourceKind::Build),
    )
    .expect_err("mutable base image");
    let remote_add = ImageBuildRequest::new(
        vec![1],
        "Dockerfile".to_owned(),
        concat!(
            "FROM php@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
            "ADD https://example.test/installer.sh /tmp/installer.sh\n"
        )
        .to_owned(),
        "linux/amd64".to_owned(),
        global_metadata(ResourceKind::Build),
    )
    .expect_err("remote ADD");

    assert_eq!(
        mutable_base.to_string(),
        "image build base 'php:8.4' must use an immutable sha256 digest"
    );
    assert_eq!(
        remote_add.to_string(),
        "image build Dockerfile must not ADD remote URLs"
    );
}

#[test]
fn image_builder_boundary_is_object_safe() {
    fn accepts_image_builder(_builder: &dyn ImageBuilder) {}

    let _ = accepts_image_builder;
}

#[test]
fn bollard_request_maps_only_typed_values_and_reserved_labels() {
    let metadata = global_metadata(ResourceKind::Gateway);
    let options = ContainerCreateOptions::new(
        "stackctl-gateway",
        concat!(
            "ghcr.io/stackctl/gateway@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ),
        metadata,
    )
    .expect("immutable container options");

    let (query, body) = create_request(&options);

    assert_eq!(query.name.as_deref(), Some("stackctl-gateway"));
    assert_eq!(body.image.as_deref(), Some(options.image()));
    assert_eq!(
        body.labels.expect("ownership labels"),
        options.metadata().labels().into_iter().collect()
    );
}

#[test]
fn published_port_discovery_uses_unfiltered_running_container_inventory() {
    let request = published_port_list_request();

    assert!(!request.all);
    assert_eq!(request.filters, None);
    assert!(!request.size);
}

#[test]
fn published_port_discovery_preserves_engine_owner_and_tcp_bindings() {
    let bindings = published_port_bindings(ContainerSummary {
        id: Some("container-1".to_owned()),
        names: Some(vec!["/legacy-proxy".to_owned()]),
        ports: Some(vec![
            PortSummary {
                ip: Some("127.0.0.1".to_owned()),
                private_port: 8080,
                public_port: Some(80),
                typ: Some(PortSummaryTypeEnum::TCP),
            },
            PortSummary {
                ip: Some("127.0.0.1".to_owned()),
                private_port: 5353,
                public_port: Some(53),
                typ: Some(PortSummaryTypeEnum::UDP),
            },
            PortSummary {
                ip: None,
                private_port: 8443,
                public_port: Some(443),
                typ: Some(PortSummaryTypeEnum::TCP),
            },
        ]),
        ..ContainerSummary::default()
    })
    .expect("published ports");

    assert_eq!(
        bindings,
        vec![
            PublishedPortBinding::new(
                "container-1",
                "legacy-proxy",
                "127.0.0.1".parse().expect("host IP"),
                80,
            )
            .expect("binding"),
            PublishedPortBinding::new(
                "container-1",
                "legacy-proxy",
                "0.0.0.0".parse().expect("unspecified host IP"),
                443,
            )
            .expect("all-interface binding"),
        ]
    );
}

#[test]
fn published_port_discovery_rejects_invalid_engine_host_addresses() {
    let error = published_port_bindings(ContainerSummary {
        id: Some("container-1".to_owned()),
        names: Some(vec!["/legacy-proxy".to_owned()]),
        ports: Some(vec![PortSummary {
            ip: Some("not-an-ip".to_owned()),
            private_port: 8080,
            public_port: Some(80),
            typ: Some(PortSummaryTypeEnum::TCP),
        }]),
        ..ContainerSummary::default()
    })
    .expect_err("invalid Engine host address");

    assert!(
        error.to_string().starts_with(
            "Engine returned invalid host IP 'not-an-ip' for container 'legacy-proxy':"
        )
    );
}

#[test]
fn published_port_discovery_is_an_object_safe_engine_capability() {
    fn accepts_published_ports(_source: &dyn PublishedPortDiscovery) {}

    let _ = accepts_published_ports;
}

#[test]
fn gateway_engine_request_has_private_network_loopback_ports_and_read_only_tls() {
    let metadata = global_metadata(ResourceKind::Gateway);
    let options = gateway_container_request(GatewayContainerRequestOptions::new(
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
    .expect("immutable gateway options");

    let (_, body) = create_request(&options);
    let host = body.host_config.expect("gateway host config");

    assert_eq!(host.network_mode.as_deref(), Some("stackctl"));
    let port_bindings = host.port_bindings.expect("loopback port bindings");
    assert_eq!(
        port_bindings
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from(["443/tcp".to_owned(), "80/tcp".to_owned()])
    );
    assert!(
        port_bindings
            .values()
            .filter_map(Option::as_ref)
            .flatten()
            .all(|binding| binding
                .host_port
                .as_deref()
                .is_some_and(|port| { port == "80" || port == "443" }))
    );
    assert_eq!(
        port_bindings
            .values()
            .filter_map(Option::as_ref)
            .flatten()
            .filter_map(|binding| binding.host_ip.clone())
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from(["127.0.0.1".to_owned(), "::1".to_owned()])
    );
    let mounts = host.mounts.expect("gateway mounts");
    assert!(mounts.iter().any(|mount| {
        mount.source.as_deref() == Some("/state/tls")
            && mount.target.as_deref() == Some("/etc/stackctl/tls")
            && mount.read_only == Some(true)
    }));
    assert!(mounts.iter().any(|mount| {
        mount.source.as_deref() == Some("/state/gateway/config.json")
            && mount.target.as_deref() == Some("/etc/stackctl/config.json")
            && mount.read_only == Some(true)
    }));
    assert!(mounts.iter().any(|mount| {
        mount.source.as_deref() == Some("/state/gateway/run")
            && mount.target.as_deref() == Some("/run/stackctl")
            && mount.read_only == Some(false)
    }));
    assert_eq!(
        body.cmd,
        Some(vec![
            "caddy".to_owned(),
            "run".to_owned(),
            "--config".to_owned(),
            "/etc/stackctl/config.json".to_owned(),
        ])
    );
    let health_check = body.healthcheck.expect("gateway health check");
    assert_eq!(
        health_check.test,
        Some(vec![
            "CMD".to_owned(),
            "caddy".to_owned(),
            "validate".to_owned(),
            "--config".to_owned(),
            "/etc/stackctl/config.json".to_owned(),
        ])
    );
    assert_eq!(health_check.interval, Some(30_000_000_000));
    assert_eq!(health_check.timeout, Some(5_000_000_000));
    assert_eq!(health_check.start_period, Some(10_000_000_000));
    assert_eq!(health_check.retries, Some(3));
    assert_eq!(health_check.start_interval, None);
    assert_eq!(
        host.restart_policy.expect("restart policy").name,
        Some(bollard::models::RestartPolicyNameEnum::UNLESS_STOPPED)
    );
}

#[test]
fn container_health_checks_reject_invalid_or_unbounded_settings() {
    let empty = ContainerHealthCheck::new(
        Vec::new(),
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_secs(1),
        1,
    )
    .expect_err("empty health command");
    let short_interval = ContainerHealthCheck::new(
        vec!["health".to_owned()],
        Duration::from_nanos(999_999),
        Duration::from_secs(1),
        Duration::from_secs(1),
        1,
    )
    .expect_err("sub-millisecond interval");
    let no_retries = ContainerHealthCheck::new(
        vec!["health".to_owned()],
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_secs(1),
        0,
    )
    .expect_err("zero retries");
    let excessive_timeout = ContainerHealthCheck::new(
        vec!["health".to_owned()],
        Duration::from_secs(1),
        Duration::MAX,
        Duration::from_secs(1),
        1,
    )
    .expect_err("unrepresentable Engine timeout");

    assert_eq!(
        empty.to_string(),
        "container health check must contain a non-empty executable and no NUL bytes"
    );
    assert_eq!(
        short_interval.to_string(),
        "container health check interval must be at least 1 millisecond"
    );
    assert_eq!(
        no_retries.to_string(),
        "container health check retries must be greater than zero"
    );
    assert_eq!(
        excessive_timeout.to_string(),
        "container health check timeout exceeds the Engine duration limit"
    );
}

#[test]
fn network_management_is_an_object_safe_owned_resource_strategy() {
    let options = NetworkCreateOptions::new("stackctl", global_metadata(ResourceKind::Network))
        .expect("managed network");
    let mut backend = RecordingNetworkBackend::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let network = runtime
        .block_on(create_network_through_strategy(&mut backend, &options))
        .expect("create network");
    runtime
        .block_on(backend.remove_network(&network))
        .expect("remove proven-owned network");

    assert_eq!(network.id().as_str(), "network-1");
    assert_eq!(backend.created, vec![options]);
    assert_eq!(backend.removed, vec![network]);
}

#[test]
fn network_requests_use_bridge_driver_and_complete_ownership_labels() {
    let metadata = global_metadata(ResourceKind::Network);
    let options = NetworkCreateOptions::new("stackctl", metadata.clone()).expect("managed network");

    let request = network_create_request(&options);

    assert_eq!(request.name, "stackctl");
    assert_eq!(request.driver.as_deref(), Some("bridge"));
    assert_eq!(
        request.labels.expect("network ownership labels"),
        metadata.labels().into_iter().collect()
    );
}

#[test]
fn network_deletion_rejects_missing_or_changed_ownership_labels() {
    let network = OwnedNetwork::new(
        NetworkId::new("network-1"),
        global_metadata(ResourceKind::Network),
    );

    let error = verify_owned_network_labels(&network, &std::collections::HashMap::new())
        .expect_err("unlabelled network");

    assert_eq!(
        error.to_string(),
        "refusing to delete network 'network-1' because its Engine ownership labels no longer match"
    );
}

#[test]
fn volume_management_requires_owned_volume_proof_for_deletion() {
    let options = VolumeCreateOptions::new(
        "stackctl-postgres-17-data",
        project_metadata(ResourceKind::Volume),
    )
    .expect("managed volume");
    let mut backend = RecordingVolumeBackend::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let volume = runtime
        .block_on(create_volume_through_strategy(&mut backend, &options))
        .expect("create volume");
    runtime
        .block_on(backend.remove_volume(&volume))
        .expect("remove proven-owned volume");

    assert_eq!(volume.name(), "stackctl-postgres-17-data");
    assert_eq!(backend.created, vec![options]);
    assert_eq!(backend.removed, vec![volume]);
}

#[test]
fn volume_requests_are_deterministic_local_volumes_with_complete_labels() {
    let metadata = project_metadata(ResourceKind::Volume);
    let options = VolumeCreateOptions::new("stackctl-postgres-17-data", metadata.clone())
        .expect("managed volume");

    let request = volume_create_request(&options);

    assert_eq!(request.name.as_deref(), Some("stackctl-postgres-17-data"));
    assert_eq!(request.driver.as_deref(), Some("local"));
    assert_eq!(
        request.labels.expect("volume ownership labels"),
        metadata.labels().into_iter().collect()
    );
}

#[test]
fn volume_deletion_rejects_missing_or_changed_ownership_labels() {
    let metadata = project_metadata(ResourceKind::Volume);
    let volume = OwnedVolume::new("stackctl-postgres-17-data", metadata);

    let error = verify_owned_volume_labels(&volume, &std::collections::HashMap::new())
        .expect_err("unlabelled volume");

    assert_eq!(
        error.to_string(),
        "refusing to delete volume 'stackctl-postgres-17-data' because its Engine ownership labels no longer match"
    );
}

#[test]
fn empty_desired_revision_is_rejected_before_resource_creation() {
    let error = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::Gateway,
        project_id: None,
        compatibility_fingerprint: "gateway-v1".to_owned(),
        schema_version: 8,
        desired_revision: String::new(),
        retention: RetentionClass::Disposable,
    })
    .expect_err("empty desired revision");

    assert_eq!(
        error.to_string(),
        "managed resource desired revision must not be empty"
    );
}

#[test]
fn complete_current_installation_labels_reconstruct_owned_metadata() {
    let metadata = project_metadata(ResourceKind::ProjectApplication);

    let ownership = classify_observed_resource(&metadata.labels(), "install-1", 8);

    assert_eq!(ownership, ObservedResourceOwnership::Owned(metadata));
}

#[test]
fn managed_container_observations_reconstruct_mutable_owned_handles() {
    let metadata = project_metadata(ResourceKind::ProjectApplication);
    let observed = ObservedContainer::new(ContainerId::new("container-1"), metadata.labels());

    let owned = reconstruct_owned_container(&observed, "install-1", 8)
        .expect("current installation container");

    assert_eq!(owned.id().as_str(), "container-1");
    assert_eq!(owned.metadata(), &metadata);
}

#[test]
fn unmanaged_container_observations_never_reconstruct_mutable_handles() {
    let observed = ObservedContainer::new(ContainerId::new("container-1"), BTreeMap::new());

    let ownership =
        reconstruct_owned_container(&observed, "install-1", 8).expect_err("unmanaged container");

    assert_eq!(ownership, ObservedResourceOwnership::Unmanaged);
}

#[test]
fn unlabelled_resources_are_never_adopted() {
    let ownership = classify_observed_resource(&BTreeMap::new(), "install-1", 8);

    assert_eq!(ownership, ObservedResourceOwnership::Unmanaged);
}

#[test]
fn foreign_installation_resources_are_never_adopted() {
    let labels = project_metadata(ResourceKind::ProjectApplication).labels();

    let ownership = classify_observed_resource(&labels, "install-2", 8);

    assert_eq!(
        ownership,
        ObservedResourceOwnership::ForeignInstallation {
            installation_id: "install-1".to_owned(),
        }
    );
}

#[test]
fn incomplete_managed_labels_are_not_treated_as_owned() {
    let mut labels = project_metadata(ResourceKind::ProjectApplication).labels();
    labels.remove("dev.stackctl.desired");

    let ownership = classify_observed_resource(&labels, "install-1", 8);

    assert_eq!(
        ownership,
        ObservedResourceOwnership::Malformed {
            detail: "managed resource label 'dev.stackctl.desired' is missing".to_owned(),
        }
    );
}

#[test]
fn unsupported_resource_schema_is_not_treated_as_owned() {
    let labels = project_metadata(ResourceKind::ProjectApplication).labels();

    let ownership = classify_observed_resource(&labels, "install-1", 9);

    assert_eq!(
        ownership,
        ObservedResourceOwnership::UnsupportedSchema {
            found: 8,
            supported: 9,
        }
    );
}

#[test]
fn managed_container_rescan_includes_stopped_objects_and_filters_by_marker() {
    let request = managed_container_list_request();

    assert!(request.all);
    assert_eq!(
        request.filters,
        Some(std::collections::HashMap::from([(
            "label".to_owned(),
            vec!["dev.stackctl.managed=true".to_owned()],
        )]))
    );
}

#[test]
fn managed_network_and_volume_rescans_filter_by_the_reserved_marker() {
    let network_request = managed_network_list_request();
    let volume_request = managed_volume_list_request();
    let expected = Some(std::collections::HashMap::from([(
        "label".to_owned(),
        vec!["dev.stackctl.managed=true".to_owned()],
    )]));

    assert_eq!(network_request.filters, expected);
    assert_eq!(volume_request.filters, expected);
}

#[test]
fn network_and_volume_discovery_are_narrow_object_safe_strategies() {
    let network = ObservedNetwork::new(
        NetworkId::new("network-1"),
        global_metadata(ResourceKind::Network).labels(),
    );
    let volume = ObservedVolume::new(
        "stackctl-postgres-17-data",
        project_metadata(ResourceKind::Volume).labels(),
    );
    let backend = RecordingResourceDiscovery {
        networks: vec![network.clone()],
        volumes: vec![volume.clone()],
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");
    let network_discovery: &dyn NetworkDiscovery = &backend;
    let volume_discovery: &dyn VolumeDiscovery = &backend;

    assert_eq!(
        runtime
            .block_on(network_discovery.discover_managed_networks())
            .expect("discover networks"),
        vec![network]
    );
    assert_eq!(
        runtime
            .block_on(volume_discovery.discover_managed_volumes())
            .expect("discover volumes"),
        vec![volume]
    );
}

#[test]
fn network_and_volume_observations_reconstruct_owned_handles_from_labels() {
    let network_metadata = global_metadata(ResourceKind::Network);
    let volume_metadata = project_metadata(ResourceKind::Volume);
    let network = observed_network(Network {
        id: Some("network-1".to_owned()),
        labels: Some(network_metadata.labels().into_iter().collect()),
        ..Network::default()
    })
    .expect("observed network");
    let volume = observed_volume(Volume {
        name: "stackctl-postgres-17-data".to_owned(),
        labels: volume_metadata.labels().into_iter().collect(),
        ..Volume::default()
    })
    .expect("observed volume");

    let owned_network = reconstruct_owned_network(&network, "install-1", 8).expect("owned network");
    let owned_volume = reconstruct_owned_volume(&volume, "install-1", 8).expect("owned volume");

    assert_eq!(owned_network.id().as_str(), "network-1");
    assert_eq!(owned_network.metadata(), &network_metadata);
    assert_eq!(owned_volume.name(), "stackctl-postgres-17-data");
    assert_eq!(owned_volume.metadata(), &volume_metadata);
}

#[test]
fn engine_container_summaries_map_to_backend_independent_observations() {
    let labels = project_metadata(ResourceKind::ProjectApplication)
        .labels()
        .into_iter()
        .collect();
    let summary = ContainerSummary {
        id: Some("container-1".to_owned()),
        labels: Some(labels),
        ..ContainerSummary::default()
    };

    let observed = observed_container(summary).expect("complete Engine summary");

    assert_eq!(observed.id().as_str(), "container-1");
    assert_eq!(
        classify_observed_resource(observed.labels(), "install-1", 8),
        ObservedResourceOwnership::Owned(project_metadata(ResourceKind::ProjectApplication))
    );
}

#[test]
fn engine_summaries_without_ids_fail_instead_of_disappearing() {
    let error = observed_container(ContainerSummary::default()).expect_err("missing ID");

    assert_eq!(
        error.to_string(),
        "Engine returned a managed container without an ID"
    );
}

#[test]
fn container_discovery_is_an_object_safe_rescan_strategy() {
    let mut backend = RecordingContainerBackend::default();
    backend.observed.push(ObservedContainer::new(
        ContainerId::new("container-1"),
        project_metadata(ResourceKind::ProjectApplication).labels(),
    ));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let observed = runtime
        .block_on(discover_through_strategy(&backend))
        .expect("discover containers");

    assert_eq!(observed, backend.observed);
}

#[test]
fn minimum_supported_engine_api_version_is_accepted() {
    validate_engine_api_version(ClientVersion {
        major_version: 1,
        minor_version: 41,
    })
    .expect("minimum supported API");
}

#[test]
fn older_engine_api_versions_fail_before_reconciliation() {
    let error = validate_engine_api_version(ClientVersion {
        major_version: 1,
        minor_version: 40,
    })
    .expect_err("unsupported Engine API");

    assert_eq!(
        error.to_string(),
        "Engine API version 1.40 is unsupported; version 1.41 or newer is required"
    );
}

#[test]
fn engine_operation_deadlines_return_structured_timeouts() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(bounded_engine_operation(
            "inspect container",
            Duration::from_millis(1),
            pending::<Result<(), super::EngineError>>(),
        ))
        .expect_err("operation deadline");

    assert_eq!(
        error,
        super::EngineError::Timeout {
            action: "inspect container".to_owned(),
            timeout_milliseconds: 1,
        }
    );
    assert_eq!(
        error.to_string(),
        "Engine operation 'inspect container' timed out after 1 ms"
    );
}

fn project_metadata(kind: ResourceKind) -> ManagedResourceMetadata {
    ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind,
        project_id: Some("bill".to_owned()),
        compatibility_fingerprint: "runtime-v1".to_owned(),
        schema_version: 8,
        desired_revision: "desired-v1".to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("valid project metadata")
}

fn global_metadata(kind: ResourceKind) -> ManagedResourceMetadata {
    ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind,
        project_id: None,
        compatibility_fingerprint: "gateway-v1".to_owned(),
        schema_version: 8,
        desired_revision: "desired-v1".to_owned(),
        retention: RetentionClass::Disposable,
    })
    .expect("valid global metadata")
}

fn create_through_strategy<'operation>(
    strategy: &'operation mut dyn ContainerLifecycle,
    options: &'operation ContainerCreateOptions,
) -> EngineFuture<'operation, OwnedContainer> {
    strategy.create(options)
}

fn discover_through_strategy(
    strategy: &dyn ContainerDiscovery,
) -> EngineFuture<'_, Vec<ObservedContainer>> {
    strategy.discover_managed()
}

fn create_network_through_strategy<'operation>(
    strategy: &'operation mut dyn NetworkManager,
    options: &'operation NetworkCreateOptions,
) -> EngineFuture<'operation, OwnedNetwork> {
    strategy.create_network(options)
}

fn create_volume_through_strategy<'operation>(
    strategy: &'operation mut dyn VolumeManager,
    options: &'operation VolumeCreateOptions,
) -> EngineFuture<'operation, OwnedVolume> {
    strategy.create_volume(options)
}

#[derive(Default)]
struct RecordingVolumeBackend {
    created: Vec<VolumeCreateOptions>,
    removed: Vec<OwnedVolume>,
}

struct RecordingResourceDiscovery {
    networks: Vec<ObservedNetwork>,
    volumes: Vec<ObservedVolume>,
}

#[derive(Default)]
struct RecordingImageResolver {
    resolved: Vec<ImmutableImageReference>,
}

struct RecordingContainerEventSource;

impl ContainerEventSource for RecordingContainerEventSource {
    fn stream_managed<'stream>(
        &'stream self,
        _installation_id: &'stream str,
        _cursor: ContainerEventCursor,
    ) -> ContainerEventStream<'stream> {
        Box::pin(futures_util::stream::once(async {
            Ok(ContainerEvent::new(
                ContainerId::new("container-1"),
                ContainerEventAction::Started,
                1,
            ))
        }))
    }
}

struct RecordingLogSource;

impl LogSource for RecordingLogSource {
    fn logs<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
        _options: &'operation ContainerLogOptions,
    ) -> EngineFuture<'operation, ContainerLogStream<'operation>> {
        Box::pin(async {
            let stream: ContainerLogStream<'operation> =
                Box::pin(futures_util::stream::once(async {
                    Ok(LogChunk::stdout(b"ready\n".to_vec()))
                }));

            Ok(stream)
        })
    }
}

impl ImageResolver for RecordingImageResolver {
    fn ensure_image<'operation>(
        &'operation mut self,
        reference: &'operation ImmutableImageReference,
    ) -> EngineFuture<'operation, ImageId> {
        Box::pin(async move {
            self.resolved.push(reference.clone());
            Ok(ImageId::new("sha256:image-config"))
        })
    }
}

impl NetworkDiscovery for RecordingResourceDiscovery {
    fn discover_managed_networks(&self) -> EngineFuture<'_, Vec<ObservedNetwork>> {
        Box::pin(async { Ok(self.networks.clone()) })
    }
}

impl VolumeDiscovery for RecordingResourceDiscovery {
    fn discover_managed_volumes(&self) -> EngineFuture<'_, Vec<ObservedVolume>> {
        Box::pin(async { Ok(self.volumes.clone()) })
    }
}

impl VolumeManager for RecordingVolumeBackend {
    fn create_volume<'operation>(
        &'operation mut self,
        options: &'operation VolumeCreateOptions,
    ) -> EngineFuture<'operation, OwnedVolume> {
        Box::pin(async move {
            self.created.push(options.clone());
            Ok(OwnedVolume::new(options.name(), options.metadata().clone()))
        })
    }

    fn remove_volume<'operation>(
        &'operation mut self,
        volume: &'operation OwnedVolume,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.removed.push(volume.clone());
            Ok(())
        })
    }
}

#[derive(Default)]
struct RecordingNetworkBackend {
    created: Vec<NetworkCreateOptions>,
    removed: Vec<OwnedNetwork>,
}

impl NetworkManager for RecordingNetworkBackend {
    fn create_network<'operation>(
        &'operation mut self,
        options: &'operation NetworkCreateOptions,
    ) -> EngineFuture<'operation, OwnedNetwork> {
        Box::pin(async move {
            self.created.push(options.clone());
            Ok(OwnedNetwork::new(
                NetworkId::new("network-1"),
                options.metadata().clone(),
            ))
        })
    }

    fn remove_network<'operation>(
        &'operation mut self,
        network: &'operation OwnedNetwork,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.removed.push(network.clone());
            Ok(())
        })
    }
}

#[derive(Default)]
struct RecordingContainerBackend {
    created: Vec<ContainerCreateOptions>,
    observed: Vec<ObservedContainer>,
}

impl ContainerDiscovery for RecordingContainerBackend {
    fn discover_managed(&self) -> EngineFuture<'_, Vec<ObservedContainer>> {
        Box::pin(async { Ok(self.observed.clone()) })
    }
}

impl ContainerLifecycle for RecordingContainerBackend {
    fn create<'operation>(
        &'operation mut self,
        options: &'operation ContainerCreateOptions,
    ) -> EngineFuture<'operation, OwnedContainer> {
        Box::pin(async move {
            self.created.push(options.clone());
            Ok(OwnedContainer::new(
                ContainerId::new("container-1"),
                options.metadata().clone(),
            ))
        })
    }

    fn start<'operation>(
        &'operation mut self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }

    fn stop<'operation>(
        &'operation mut self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }

    fn remove<'operation>(
        &'operation mut self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }

    fn inspect<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ContainerState> {
        Box::pin(async { Ok(ContainerState::Running) })
    }
}
