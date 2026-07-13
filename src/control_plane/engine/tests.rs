use super::{
    ContainerCreateOptions, ContainerId, ContainerLifecycle, ContainerState, EngineError,
    ManagedResourceMetadata, ResourceKind,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[test]
fn managed_metadata_generates_complete_reserved_ownership_labels() {
    let metadata = ManagedResourceMetadata::new(
        "install-1",
        ResourceKind::SharedService,
        Some(PathBuf::from("/work/bill")),
        Some("sha256:abc123".to_owned()),
    )
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
            ("dev.stackctl.project".to_owned(), "/work/bill".to_owned()),
        ])
    );
}

#[test]
fn container_lifecycle_is_an_object_safe_replaceable_strategy() {
    let metadata = ManagedResourceMetadata::new(
        "install-1",
        ResourceKind::ProjectApplication,
        Some(PathBuf::from("/work/bill")),
        None,
    )
    .expect("valid managed metadata");
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

    let id = create_through_strategy(&mut backend, &options).expect("create container");

    assert_eq!(id.as_str(), "container-1");
    assert_eq!(backend.created, vec![options]);
}

#[test]
fn mutable_image_tags_are_rejected_before_an_engine_request() {
    let metadata = ManagedResourceMetadata::new(
        "install-1",
        ResourceKind::Gateway,
        None,
        Some("gateway-v1".to_owned()),
    )
    .expect("valid managed metadata");

    let error = ContainerCreateOptions::new("stackctl-gateway", "caddy:latest", metadata)
        .expect_err("mutable image reference");

    assert_eq!(
        error.to_string(),
        "managed image 'caddy:latest' must use an immutable sha256 digest"
    );
}

fn create_through_strategy(
    strategy: &mut dyn ContainerLifecycle,
    options: &ContainerCreateOptions,
) -> Result<ContainerId, EngineError> {
    strategy.create(options)
}

#[derive(Default)]
struct RecordingContainerBackend {
    created: Vec<ContainerCreateOptions>,
}

impl ContainerLifecycle for RecordingContainerBackend {
    fn create(&mut self, options: &ContainerCreateOptions) -> Result<ContainerId, EngineError> {
        self.created.push(options.clone());

        Ok(ContainerId::new("container-1"))
    }

    fn start(&mut self, _container: &ContainerId) -> Result<(), EngineError> {
        Ok(())
    }

    fn stop(&mut self, _container: &ContainerId) -> Result<(), EngineError> {
        Ok(())
    }

    fn remove(&mut self, _container: &ContainerId) -> Result<(), EngineError> {
        Ok(())
    }

    fn inspect(&self, _container: &ContainerId) -> Result<ContainerState, EngineError> {
        Ok(ContainerState::Running)
    }
}
