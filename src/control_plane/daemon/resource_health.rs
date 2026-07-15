use crate::control_plane::engine::ContainerHealth;

/// Live health evidence for one exact managed Engine resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResourceHealth {
    Container(ContainerHealth),
    ServiceNotReady { attempt: u32 },
    AuthenticationFailed { attempt: u32 },
    LogicalResourceDrift,
    GatewayRouteDrift,
    DestructiveReplacementRequired,
}

impl From<ContainerHealth> for ResourceHealth {
    fn from(health: ContainerHealth) -> Self {
        Self::Container(health)
    }
}
