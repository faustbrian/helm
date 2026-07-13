use super::{ContainerCreateOptions, ContainerId, ContainerState, EngineError};

/// Narrow container lifecycle effects implemented by Docker and Podman adapters.
pub(crate) trait ContainerLifecycle {
    fn create(&mut self, options: &ContainerCreateOptions) -> Result<ContainerId, EngineError>;

    fn start(&mut self, container: &ContainerId) -> Result<(), EngineError>;

    fn stop(&mut self, container: &ContainerId) -> Result<(), EngineError>;

    fn remove(&mut self, container: &ContainerId) -> Result<(), EngineError>;

    fn inspect(&self, container: &ContainerId) -> Result<ContainerState, EngineError>;
}
