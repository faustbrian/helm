use super::V7PublicFileArtifact;
use crate::control_plane::state::AcceptedV7InventoryRecord;
use crate::control_plane::tls::CertificateTrustStore;
use std::path::Path;

/// Accepted legacy CAs and the installation-owned Stackctl CA transition.
pub(crate) struct V7InstallationTrustMigrationAdapterOptions<'operation> {
    pub(crate) accepted: &'operation AcceptedV7InventoryRecord,
    pub(crate) legacy_ca_artifacts: &'operation [V7PublicFileArtifact],
    pub(crate) target_certificate_path: &'operation Path,
    pub(crate) trust_store: &'operation (dyn CertificateTrustStore + Sync),
    pub(crate) backup_root: &'operation Path,
    pub(crate) created_at_unix_seconds: i64,
    pub(crate) verified_at_unix_seconds: i64,
}
