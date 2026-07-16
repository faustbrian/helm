use super::{
    BindMount, ContainerHealthCheck, ContainerRestartPolicy, EngineError, ManagedResourceMetadata,
    PortBinding, TmpfsMount, VolumeMount, is_immutable_image_identity,
    is_valid_container_environment_key,
};
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};

/// Typed options required to create one owned container.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct ContainerCreateOptions {
    name: String,
    image: String,
    metadata: ManagedResourceMetadata,
    user: Option<String>,
    platform: Option<String>,
    network: Option<String>,
    port_bindings: Vec<PortBinding>,
    bind_mounts: Vec<BindMount>,
    volume_mounts: Vec<VolumeMount>,
    tmpfs_mounts: Vec<TmpfsMount>,
    shared_memory_bytes: Option<i64>,
    working_directory: Option<String>,
    command: Vec<String>,
    environment: BTreeMap<String, String>,
    health_check: Option<ContainerHealthCheck>,
    image_health_check_disabled: bool,
    restart_policy: Option<ContainerRestartPolicy>,
    linux_capabilities_disabled: bool,
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

        if !is_immutable_image_identity(&image) {
            return Err(EngineError::InvalidRequest {
                detail: format!("managed image '{image}' must use an immutable sha256 digest"),
            });
        }

        Ok(Self {
            name,
            image,
            metadata,
            user: None,
            platform: None,
            network: None,
            port_bindings: Vec::new(),
            bind_mounts: Vec::new(),
            volume_mounts: Vec::new(),
            tmpfs_mounts: Vec::new(),
            shared_memory_bytes: None,
            working_directory: None,
            command: Vec::new(),
            environment: BTreeMap::new(),
            health_check: None,
            image_health_check_disabled: false,
            restart_policy: None,
            linux_capabilities_disabled: false,
        })
    }

    /// Runs the container with one explicit numeric Linux UID and GID.
    pub(crate) fn with_user(mut self, user: impl Into<String>) -> Result<Self, EngineError> {
        let user = user.into();
        let valid = user.split_once(':').is_some_and(|(uid, gid)| {
            !uid.is_empty()
                && !gid.is_empty()
                && uid.bytes().all(|byte| byte.is_ascii_digit())
                && gid.bytes().all(|byte| byte.is_ascii_digit())
        });
        if !valid {
            return Err(EngineError::InvalidRequest {
                detail: format!(
                    "managed container user '{user}' must be a numeric Linux UID:GID pair"
                ),
            });
        }
        self.user = Some(user);

        Ok(self)
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

    /// Replaces a preflight image with a validated derived Engine identity.
    pub(crate) fn with_image(mut self, image: impl Into<String>) -> Result<Self, EngineError> {
        let image = image.into();
        if !is_immutable_image_identity(&image) {
            return Err(EngineError::InvalidRequest {
                detail: format!("managed image '{image}' must use an immutable sha256 digest"),
            });
        }
        self.image = image;

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

    pub(crate) fn with_tmpfs_mount(mut self, mount: TmpfsMount) -> Self {
        self.tmpfs_mounts.push(mount);
        self
    }

    /// Sets a bounded Linux `/dev/shm` size for memory-intensive workloads.
    pub(crate) fn with_shared_memory_bytes(mut self, bytes: u64) -> Result<Self, EngineError> {
        if bytes == 0 || bytes > i64::MAX as u64 {
            return Err(EngineError::InvalidRequest {
                detail: "managed container shared memory must fit a positive Engine byte range"
                    .to_owned(),
            });
        }
        self.shared_memory_bytes = Some(bytes as i64);

        Ok(self)
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

    pub(crate) fn with_working_directory(
        mut self,
        directory: impl Into<String>,
    ) -> Result<Self, EngineError> {
        let directory = directory.into();
        if !directory.starts_with('/') || directory.contains(['\0', '\n', '\r']) {
            return Err(EngineError::InvalidRequest {
                detail: format!(
                    "managed container working directory '{directory}' must be an absolute path without control characters"
                ),
            });
        }
        self.working_directory = Some(directory);

        Ok(self)
    }

    pub(crate) fn with_environment(
        mut self,
        environment: BTreeMap<String, String>,
    ) -> Result<Self, EngineError> {
        for (key, value) in &environment {
            if !is_valid_container_environment_key(key) {
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

    /// Drops the complete default Linux capability set for this container.
    pub(crate) const fn without_linux_capabilities(mut self) -> Self {
        self.linux_capabilities_disabled = true;
        self
    }

    pub(crate) fn with_health_check(mut self, health_check: ContainerHealthCheck) -> Self {
        self.health_check = Some(health_check);
        self.image_health_check_disabled = false;
        self
    }

    pub(crate) fn without_image_health_check(mut self) -> Self {
        self.health_check = None;
        self.image_health_check_disabled = true;
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

    pub(crate) fn user(&self) -> Option<&str> {
        self.user.as_deref()
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

    pub(crate) fn tmpfs_mounts(&self) -> &[TmpfsMount] {
        &self.tmpfs_mounts
    }

    pub(crate) const fn shared_memory_bytes(&self) -> Option<i64> {
        self.shared_memory_bytes
    }

    pub(crate) fn command(&self) -> &[String] {
        &self.command
    }

    pub(crate) fn working_directory(&self) -> Option<&str> {
        self.working_directory.as_deref()
    }

    pub(crate) const fn environment(&self) -> &BTreeMap<String, String> {
        &self.environment
    }

    pub(crate) const fn restart_policy(&self) -> Option<ContainerRestartPolicy> {
        self.restart_policy
    }

    pub(crate) const fn health_check(&self) -> Option<&ContainerHealthCheck> {
        self.health_check.as_ref()
    }

    pub(crate) const fn image_health_check_disabled(&self) -> bool {
        self.image_health_check_disabled
    }

    pub(crate) const fn linux_capabilities_disabled(&self) -> bool {
        self.linux_capabilities_disabled
    }
}

impl Debug for ContainerCreateOptions {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ContainerCreateOptions")
            .field("name", &self.name)
            .field("image", &self.image)
            .field("metadata", &self.metadata)
            .field("user", &self.user)
            .field("platform", &self.platform)
            .field("network", &self.network)
            .field("port_bindings", &self.port_bindings)
            .field("bind_mounts", &self.bind_mounts)
            .field("volume_mounts", &self.volume_mounts)
            .field("tmpfs_mounts", &self.tmpfs_mounts)
            .field("shared_memory_bytes", &self.shared_memory_bytes)
            .field("working_directory", &self.working_directory)
            .field("command", &self.command)
            .field("environment_keys", &self.environment.keys())
            .field("health_check", &self.health_check)
            .field(
                "image_health_check_disabled",
                &self.image_health_check_disabled,
            )
            .field("restart_policy", &self.restart_policy)
            .field(
                "linux_capabilities_disabled",
                &self.linux_capabilities_disabled,
            )
            .finish()
    }
}
