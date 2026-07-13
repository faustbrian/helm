use super::{
    BindMount, ContainerRestartPolicy, EngineError, ImmutableImageReference,
    ManagedResourceMetadata, PortBinding, VolumeMount,
};
use crate::control_plane::is_valid_environment_variable_key;
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};

/// Typed options required to create one owned container.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct ContainerCreateOptions {
    name: String,
    image: String,
    metadata: ManagedResourceMetadata,
    platform: Option<String>,
    network: Option<String>,
    port_bindings: Vec<PortBinding>,
    bind_mounts: Vec<BindMount>,
    volume_mounts: Vec<VolumeMount>,
    command: Vec<String>,
    environment: BTreeMap<String, String>,
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
            platform: None,
            network: None,
            port_bindings: Vec::new(),
            bind_mounts: Vec::new(),
            volume_mounts: Vec::new(),
            command: Vec::new(),
            environment: BTreeMap::new(),
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

    pub(crate) fn with_platform(
        mut self,
        platform: impl Into<String>,
    ) -> Result<Self, EngineError> {
        let platform = platform.into();
        let segments = platform.split('/').collect::<Vec<_>>();

        if !matches!(segments.as_slice(), ["linux", architecture] if !architecture.is_empty())
            && !matches!(
                segments.as_slice(),
                ["linux", architecture, variant]
                    if !architecture.is_empty() && !variant.is_empty()
            )
        {
            return Err(EngineError::InvalidRequest {
                detail: format!(
                    "managed container platform '{platform}' must identify a Linux architecture"
                ),
            });
        }

        self.platform = Some(platform);

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

    pub(crate) fn with_volume_mount(mut self, mount: VolumeMount) -> Self {
        self.volume_mounts.push(mount);
        self
    }

    pub(crate) fn with_command(mut self, command: Vec<String>) -> Result<Self, EngineError> {
        if command.first().is_none_or(String::is_empty)
            || command.iter().any(|argument| argument.contains('\0'))
        {
            return Err(EngineError::InvalidRequest {
                detail:
                    "managed container command must contain a non-empty executable and no NUL bytes"
                        .to_owned(),
            });
        }

        self.command = command;

        Ok(self)
    }

    pub(crate) fn with_environment(
        mut self,
        environment: BTreeMap<String, String>,
    ) -> Result<Self, EngineError> {
        for (key, value) in &environment {
            if !is_valid_environment_variable_key(key) {
                return Err(EngineError::InvalidRequest {
                    detail: format!("managed container environment key '{key}' is invalid"),
                });
            }
            if value.contains('\0') {
                return Err(EngineError::InvalidRequest {
                    detail: format!(
                        "managed container environment value for '{key}' must not contain NUL bytes"
                    ),
                });
            }
        }

        self.environment = environment;

        Ok(self)
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

    pub(crate) fn platform(&self) -> Option<&str> {
        self.platform.as_deref()
    }

    pub(crate) fn network(&self) -> Option<&str> {
        self.network.as_deref()
    }

    pub(crate) fn port_bindings(&self) -> &[PortBinding] {
        &self.port_bindings
    }

    pub(crate) fn bind_mounts(&self) -> &[BindMount] {
        &self.bind_mounts
    }

    pub(crate) fn volume_mounts(&self) -> &[VolumeMount] {
        &self.volume_mounts
    }

    pub(crate) fn command(&self) -> &[String] {
        &self.command
    }

    pub(crate) const fn environment(&self) -> &BTreeMap<String, String> {
        &self.environment
    }

    pub(crate) const fn restart_policy(&self) -> Option<ContainerRestartPolicy> {
        self.restart_policy
    }
}

impl Debug for ContainerCreateOptions {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ContainerCreateOptions")
            .field("name", &self.name)
            .field("image", &self.image)
            .field("metadata", &self.metadata)
            .field("platform", &self.platform)
            .field("network", &self.network)
            .field("port_bindings", &self.port_bindings)
            .field("bind_mounts", &self.bind_mounts)
            .field("volume_mounts", &self.volume_mounts)
            .field("command", &self.command)
            .field("environment_keys", &self.environment.keys())
            .field("restart_policy", &self.restart_policy)
            .finish()
    }
}
