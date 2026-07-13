use super::{
    ContainerCreateOptions, ContainerId, ContainerLifecycle, ContainerState, EngineFuture,
    ManagedResourceMetadata, ManagedResourceMetadataOptions, ObservedResourceOwnership,
    ResourceKind, RetentionClass, classify_observed_resource,
};
use std::collections::BTreeMap;

use super::bollard_engine_adapter::create_request;

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
    let id = runtime
        .block_on(create_through_strategy(&mut backend, &options))
        .expect("create container");

    assert_eq!(id.as_str(), "container-1");
    assert_eq!(backend.created, vec![options]);
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
) -> EngineFuture<'operation, ContainerId> {
    strategy.create(options)
}

#[derive(Default)]
struct RecordingContainerBackend {
    created: Vec<ContainerCreateOptions>,
}

impl ContainerLifecycle for RecordingContainerBackend {
    fn create<'operation>(
        &'operation mut self,
        options: &'operation ContainerCreateOptions,
    ) -> EngineFuture<'operation, ContainerId> {
        Box::pin(async move {
            self.created.push(options.clone());

            Ok(ContainerId::new("container-1"))
        })
    }

    fn start<'operation>(
        &'operation mut self,
        _container: &'operation ContainerId,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }

    fn stop<'operation>(
        &'operation mut self,
        _container: &'operation ContainerId,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }

    fn remove<'operation>(
        &'operation mut self,
        _container: &'operation ContainerId,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }

    fn inspect<'operation>(
        &'operation self,
        _container: &'operation ContainerId,
    ) -> EngineFuture<'operation, ContainerState> {
        Box::pin(async { Ok(ContainerState::Running) })
    }
}
