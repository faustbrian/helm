use super::BackupVerificationError;
use crate::control_plane::state::ResourceRecord;
use serde::Serialize;
use sha2::{Digest, Sha256};

/// Portable identity and checksum metadata written beside one backup artifact.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct BackupArtifactManifest {
    schema_version: u32,
    resource_id: String,
    installation_id: String,
    compatibility_fingerprint: String,
    artifact_sha256: String,
    created_at_unix_seconds: i64,
}

impl BackupArtifactManifest {
    pub(crate) fn from_artifact(
        resource: &ResourceRecord,
        artifact: &[u8],
        created_at_unix_seconds: i64,
    ) -> Result<Self, BackupVerificationError> {
        if artifact.is_empty() {
            return Err(BackupVerificationError::EmptyArtifact);
        }
        if created_at_unix_seconds < 0 {
            return Err(BackupVerificationError::InvalidCreationTime);
        }

        Ok(Self {
            schema_version: 1,
            resource_id: resource.resource_id().to_owned(),
            installation_id: resource.installation_id().to_owned(),
            compatibility_fingerprint: resource.compatibility_fingerprint().to_owned(),
            artifact_sha256: hex::encode(Sha256::digest(artifact)),
            created_at_unix_seconds,
        })
    }

    pub(super) fn resource_id(&self) -> &str {
        &self.resource_id
    }

    pub(super) fn installation_id(&self) -> &str {
        &self.installation_id
    }

    pub(super) fn compatibility_fingerprint(&self) -> &str {
        &self.compatibility_fingerprint
    }

    pub(super) fn artifact_sha256(&self) -> &str {
        &self.artifact_sha256
    }

    pub(super) const fn created_at_unix_seconds(&self) -> i64 {
        self.created_at_unix_seconds
    }
}
