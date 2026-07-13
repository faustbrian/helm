use std::path::Path;
use time::OffsetDateTime;

/// Complete host identity, location, and time for gateway asset preparation.
#[derive(Clone, Copy)]
pub(crate) struct GatewayRuntimeAssetOptions<'operation> {
    pub(crate) runtime_directory: &'operation Path,
    pub(crate) installation_id: &'operation str,
    pub(crate) container_user: &'operation str,
    pub(crate) now: OffsetDateTime,
}
