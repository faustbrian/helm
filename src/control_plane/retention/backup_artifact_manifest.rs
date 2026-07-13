use super::BackupVerificationError;
use crate::control_plane::state::ResourceRecord;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Portable identity and checksum metadata written beside one backup artifact.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BackupArtifactManifest {
    schema_version: u32,
    resource_id: String,
    installation_id: String,
    resource_kind: String,
    compatibility_fingerprint: String,
    artifact_sha256: String,
    artifact_size_bytes: u64,
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

        Self::from_checksum(
            resource,
            hex::encode(Sha256::digest(artifact)),
            u64::try_from(artifact.len()).map_err(|_| {
                BackupVerificationError::InvalidManifest {
                    detail: "backup artifact size exceeds the supported range".to_owned(),
                }
            })?,
            created_at_unix_seconds,
        )
    }

    pub(super) fn from_checksum(
        resource: &ResourceRecord,
        artifact_sha256: String,
        artifact_size_bytes: u64,
        created_at_unix_seconds: i64,
    ) -> Result<Self, BackupVerificationError> {
        let manifest = Self {
            schema_version: 1,
            resource_id: resource.resource_id().to_owned(),
            installation_id: resource.installation_id().to_owned(),
            resource_kind: resource.kind().to_owned(),
            compatibility_fingerprint: resource.compatibility_fingerprint().to_owned(),
            artifact_sha256,
            artifact_size_bytes,
            created_at_unix_seconds,
        };
        manifest.validate()?;

        Ok(manifest)
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

    pub(super) fn resource_kind(&self) -> &str {
        &self.resource_kind
    }

    pub(super) fn artifact_sha256(&self) -> &str {
        &self.artifact_sha256
    }

    pub(super) const fn artifact_size_bytes(&self) -> u64 {
        self.artifact_size_bytes
    }

    pub(super) const fn created_at_unix_seconds(&self) -> i64 {
        self.created_at_unix_seconds
    }

    pub(super) fn validate(&self) -> Result<(), BackupVerificationError> {
        if self.schema_version != 1 {
            return Err(BackupVerificationError::InvalidManifest {
                detail: format!(
                    "backup manifest schema version {} is unsupported",
                    self.schema_version
                ),
            });
        }
        if [
            self.resource_id.as_str(),
            self.installation_id.as_str(),
            self.resource_kind.as_str(),
            self.compatibility_fingerprint.as_str(),
        ]
        .into_iter()
        .any(str::is_empty)
        {
            return Err(BackupVerificationError::InvalidManifest {
                detail: "backup manifest resource identity must not be empty".to_owned(),
            });
        }
        if self.artifact_sha256.len() != 64
            || !self
                .artifact_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(BackupVerificationError::InvalidManifest {
                detail: "backup manifest artifact checksum must be lowercase SHA-256".to_owned(),
            });
        }
        if self.artifact_size_bytes == 0 {
            return Err(BackupVerificationError::EmptyArtifact);
        }
        if self.created_at_unix_seconds < 0 {
            return Err(BackupVerificationError::InvalidCreationTime);
        }

        Ok(())
    }
}
