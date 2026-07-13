use super::{
    BindMount, ContainerRestartPolicy, EngineError, ImmutableImageReference,
    ManagedResourceMetadata, PortBinding,
};

/// Typed options required to create one owned container.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ContainerCreateOptions {
    name: String,
    image: String,
    metadata: ManagedResourceMetadata,
    network: Option<String>,
    port_bindings: Vec<PortBinding>,
    bind_mounts: Vec<BindMount>,
    restart_policy: Option<ContainerRestartPolicy>,
}

impl ContainerCreateOptions {
    /// Creates options only for an immutable sha256 image reference.
    pub(crate) fn new(
        name: impl Into<String>,
        image: impl Into<String>,
        metadata: ManagedResourceMetadata,
    ) -> Result<Self, EngineError> {
        let name = name.into();
        let image = image.into();

        if name.is_empty() {
            return Err(EngineError::InvalidRequest {
                detail: "managed container name must not be empty".to_owned(),
            });
        }

        ImmutableImageReference::new(&image)?;

        Ok(Self {
            name,
            image,
            metadata,
            network: None,
            port_bindings: Vec::new(),
            bind_mounts: Vec::new(),
            restart_policy: None,
        })
    }

    pub(crate) fn with_network(mut self, network: impl Into<String>) -> Result<Self, EngineError> {
        let network = network.into();

        if network.is_empty() {
            return Err(EngineError::InvalidRequest {
                detail: "managed container network must not be empty".to_owned(),
            });
        }

        self.network = Some(network);

        Ok(self)
    }

    pub(crate) fn with_port_binding(mut self, binding: PortBinding) -> Self {
        self.port_bindings.push(binding);
        self
    }

    pub(crate) fn with_bind_mount(mut self, mount: BindMount) -> Self {
        self.bind_mounts.push(mount);
        self
    }

    pub(crate) fn with_restart_policy(mut self, policy: ContainerRestartPolicy) -> Self {
        self.restart_policy = Some(policy);
        self
    }

    /// Returns the exact engine resource name.
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// Returns the immutable image reference.
    pub(crate) fn image(&self) -> &str {
        &self.image
    }

    /// Returns mandatory ownership metadata.
    pub(crate) const fn metadata(&self) -> &ManagedResourceMetadata {
        &self.metadata
    }

    pub(super) fn network(&self) -> Option<&str> {
        self.network.as_deref()
    }

    pub(super) fn port_bindings(&self) -> &[PortBinding] {
        &self.port_bindings
    }

    pub(super) fn bind_mounts(&self) -> &[BindMount] {
        &self.bind_mounts
    }

    pub(super) const fn restart_policy(&self) -> Option<ContainerRestartPolicy> {
        self.restart_policy
    }
}
