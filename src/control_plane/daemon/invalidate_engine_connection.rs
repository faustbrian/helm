use std::time::Instant;

use super::{EngineConnectionSupervisor, EngineConnector, ResourceHealthRegistry, RetryDelay};

/// Drops an unusable Engine adapter and every observation obtained through it.
pub(crate) fn invalidate_engine_connection<Connector>(
    connection: &mut EngineConnectionSupervisor<Connector>,
    resource_health: &mut ResourceHealthRegistry,
    now: Instant,
) -> RetryDelay
where
    Connector: EngineConnector,
{
    resource_health.clear();
    connection.invalidate(now)
}
