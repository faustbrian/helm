use crate::control_plane::state::ResourceRecord;

/// Checksum proof bound to the exact persistent resource it protects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VerifiedBackupEvidence {
    resource_id: String,
    installation_id: String,
    compatibility_fingerprint: String,
    artifact_sha256: String,
    verified_at_unix_seconds: i64,
}

impl VerifiedBackupEvidence {
    pub(super) fn new(
        resource_id: String,
        installation_id: String,
        compatibility_fingerprint: String,
        artifact_sha256: String,
        verified_at_unix_seconds: i64,
    ) -> Self {
        Self {
            resource_id,
            installation_id,
            compatibility_fingerprint,
            artifact_sha256,
            verified_at_unix_seconds,
        }
    }

    pub(super) fn matches(&self, resource: &ResourceRecord) -> bool {
        self.resource_id == resource.resource_id()
            && self.installation_id == resource.installation_id()
            && self.compatibility_fingerprint == resource.compatibility_fingerprint()
            && !self.artifact_sha256.is_empty()
            && self.verified_at_unix_seconds >= 0
    }
}
