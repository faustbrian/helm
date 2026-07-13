use super::EngineError;

/// One TCP port deliberately published on host loopback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PortBinding {
    host_port: u16,
    container_port: u16,
}

impl PortBinding {
    pub(crate) fn loopback(host_port: u16, container_port: u16) -> Result<Self, EngineError> {
        if host_port == 0 || container_port == 0 {
            return Err(EngineError::InvalidRequest {
                detail: "published ports must be non-zero".to_owned(),
            });
        }

        Ok(Self {
            host_port,
            container_port,
        })
    }

    pub(super) const fn host_port(self) -> u16 {
        self.host_port
    }

    pub(super) const fn container_port(self) -> u16 {
        self.container_port
    }
}
