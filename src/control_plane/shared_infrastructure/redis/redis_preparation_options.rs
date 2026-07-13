use std::path::Path;

/// Stable installation and filesystem scope for Redis-compatible demand.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RedisPreparationOptions<'operation> {
    pub(crate) installation_id: &'operation str,
    pub(crate) network_name: &'operation str,
    pub(crate) schema_version: u32,
    pub(crate) state_directory: &'operation Path,
}
