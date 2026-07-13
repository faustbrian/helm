use super::{ContainerId, EngineError};
use std::net::IpAddr;

/// Backend-independent identity for one running container's public TCP port.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PublishedPortBinding {
    container_id: ContainerId,
    container_name: String,
    host_ip: IpAddr,
    host_port: u16,
}

impl PublishedPortBinding {
    pub(crate) fn new(
        container_id: impl Into<String>,
        container_name: impl Into<String>,
        host_ip: IpAddr,
        host_port: u16,
    ) -> Result<Self, EngineError> {
        let container_id = container_id.into();
        let container_name = container_name.into();

        if container_id.is_empty() || container_name.is_empty() || host_port == 0 {
            return Err(EngineError::Backend {
                detail: "Engine returned an incomplete published TCP port binding".to_owned(),
            });
        }

        Ok(Self {
            container_id: ContainerId::new(container_id),
            container_name,
            host_ip,
            host_port,
        })
    }

    pub(crate) const fn container_id(&self) -> &ContainerId {
        &self.container_id
    }

    pub(crate) fn container_name(&self) -> &str {
        &self.container_name
    }

    pub(crate) const fn host_ip(&self) -> IpAddr {
        self.host_ip
    }

    pub(crate) const fn host_port(&self) -> u16 {
        self.host_port
    }
}
