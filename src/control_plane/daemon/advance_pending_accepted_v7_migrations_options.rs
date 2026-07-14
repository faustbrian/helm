use super::EngineReconciliationPlan;
use crate::control_plane::application::ControlPlane;
use crate::control_plane::gateway::{GatewayConfiguration, GatewaySnapshot};
use crate::control_plane::shared_infrastructure::PreparedSharedInstance;
use crate::control_plane::state::StateStore;
use crate::control_plane::tls::CertificateTrustStore;
use std::path::Path;
use std::time::Duration;

/// Reconciled target capabilities required to advance every pending v7 plan.
pub(crate) struct AdvancePendingAcceptedV7MigrationsOptions<'operation, Store, E>
where
    Store: StateStore,
{
    pub(crate) control_plane: &'operation mut ControlPlane<Store>,
    pub(crate) engine: &'operation E,
    pub(crate) reconciliation: &'operation EngineReconciliationPlan,
    pub(crate) prepared_shared: &'operation [PreparedSharedInstance],
    pub(crate) gateway_provider: &'operation mut dyn GatewayConfiguration,
    pub(crate) target_gateway: &'operation GatewaySnapshot,
    pub(crate) trust_store: &'operation (dyn CertificateTrustStore + Sync),
    pub(crate) target_certificate_path: &'operation Path,
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
    pub(crate) backup_root: &'operation Path,
    pub(crate) maximum_config_bytes: usize,
    pub(crate) updated_at_unix_seconds: i64,
    pub(crate) timeout: Duration,
}
