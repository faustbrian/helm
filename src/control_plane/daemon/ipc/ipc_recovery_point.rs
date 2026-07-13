use crate::control_plane::state::RecoveryPointRecord;
use serde::{Deserialize, Serialize};

/// Typed immutable recovery evidence exposed over the user-only IPC channel.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcRecoveryPoint {
    recovery_point_id: String,
    service: String,
    recovery_point: String,
    artifact_sha256: String,
    artifact_size_bytes: u64,
    created_at_unix_seconds: i64,
    verified_at_unix_seconds: i64,
}

impl From<&RecoveryPointRecord> for IpcRecoveryPoint {
    fn from(point: &RecoveryPointRecord) -> Self {
        Self {
            recovery_point_id: point.recovery_point_id().to_owned(),
            service: point.service_id().to_owned(),
            recovery_point: point.reference().to_owned(),
            artifact_sha256: point.artifact_sha256().to_owned(),
            artifact_size_bytes: point.artifact_size_bytes(),
            created_at_unix_seconds: point.created_at_unix_seconds(),
            verified_at_unix_seconds: point.verified_at_unix_seconds(),
        }
    }
}

impl IpcRecoveryPoint {
    pub(crate) fn recovery_point_id(&self) -> &str {
        &self.recovery_point_id
    }

    pub(crate) fn service(&self) -> &str {
        &self.service
    }

    pub(crate) fn recovery_point(&self) -> &str {
        &self.recovery_point
    }

    pub(crate) fn artifact_sha256(&self) -> &str {
        &self.artifact_sha256
    }

    pub(crate) const fn artifact_size_bytes(&self) -> u64 {
        self.artifact_size_bytes
    }

    pub(crate) const fn created_at_unix_seconds(&self) -> i64 {
        self.created_at_unix_seconds
    }

    pub(crate) const fn verified_at_unix_seconds(&self) -> i64 {
        self.verified_at_unix_seconds
    }
}
