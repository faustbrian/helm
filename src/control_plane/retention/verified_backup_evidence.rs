use super::BackupResourceIdentity;
use crate::control_plane::state::ResourceRecord;

/// Checksum proof bound to the exact persistent resource it protects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VerifiedBackupEvidence {
    resource_id: String,
    installation_id: String,
    resource_kind: String,
    compatibility_fingerprint: String,
    artifact_sha256: String,
    artifact_size_bytes: u64,
    verified_at_unix_seconds: i64,
}

impl VerifiedBackupEvidence {
    pub(super) fn new(
        resource_id: String,
        installation_id: String,
        resource_kind: String,
        compatibility_fingerprint: String,
        artifact_sha256: String,
        artifact_size_bytes: u64,
        verified_at_unix_seconds: i64,
    ) -> Self {
        Self {
            resource_id,
            installation_id,
            resource_kind,
            compatibility_fingerprint,
            artifact_sha256,
            artifact_size_bytes,
            verified_at_unix_seconds,
        }
    }

    pub(super) fn matches(&self, resource: &ResourceRecord) -> bool {
        self.matches_identity(&BackupResourceIdentity::from_resource(resource))
    }

    pub(crate) fn matches_identity(&self, resource: &BackupResourceIdentity) -> bool {
        self.resource_id == resource.resource_id()
            && self.installation_id == resource.installation_id()
            && self.resource_kind == resource.resource_kind()
            && self.compatibility_fingerprint == resource.compatibility_fingerprint()
            && !self.artifact_sha256.is_empty()
            && self.verified_at_unix_seconds >= 0
    }

    pub(crate) fn artifact_sha256(&self) -> &str {
        &self.artifact_sha256
    }

    pub(crate) const fn artifact_size_bytes(&self) -> u64 {
        self.artifact_size_bytes
    }
}
