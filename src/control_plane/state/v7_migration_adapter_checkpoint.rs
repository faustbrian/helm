use serde::{Deserialize, Serialize};

/// Last durable preparation or cutover step for one selected adapter.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum V7MigrationAdapterCheckpointPhase {
    Pending,
    RecoveryVerified,
    TargetVerified,
    Cutover,
    Confirmed,
    RolledBack,
}

/// Secret-free, append-only recovery and target proof for one adapter.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct V7MigrationAdapterCheckpoint {
    adapter_id: String,
    adapter_kind: String,
    requires_recovery: bool,
    phase: V7MigrationAdapterCheckpointPhase,
    recovery_reference: Option<String>,
    recovery_artifact_sha256: Option<String>,
    recovery_artifact_size_bytes: Option<u64>,
    target_reference: Option<String>,
    updated_at_unix_seconds: i64,
}

impl V7MigrationAdapterCheckpoint {
    pub(crate) fn pending(
        adapter_id: impl Into<String>,
        adapter_kind: impl Into<String>,
        requires_recovery: bool,
        updated_at_unix_seconds: i64,
    ) -> Result<Self, String> {
        let checkpoint = Self {
            adapter_id: adapter_id.into(),
            adapter_kind: adapter_kind.into(),
            requires_recovery,
            phase: V7MigrationAdapterCheckpointPhase::Pending,
            recovery_reference: None,
            recovery_artifact_sha256: None,
            recovery_artifact_size_bytes: None,
            target_reference: None,
            updated_at_unix_seconds,
        };
        checkpoint.validate()?;

        Ok(checkpoint)
    }

    pub(crate) fn with_recovery_verified(
        mut self,
        reference: impl Into<String>,
        artifact_sha256: impl Into<String>,
        artifact_size_bytes: u64,
        updated_at_unix_seconds: i64,
    ) -> Result<Self, String> {
        if self.phase != V7MigrationAdapterCheckpointPhase::Pending {
            return Err(format!(
                "v7 migration adapter '{}' cannot verify recovery from a non-pending phase",
                self.adapter_id
            ));
        }
        self.phase = V7MigrationAdapterCheckpointPhase::RecoveryVerified;
        self.recovery_reference = Some(reference.into());
        self.recovery_artifact_sha256 = Some(artifact_sha256.into());
        self.recovery_artifact_size_bytes = Some(artifact_size_bytes);
        self.updated_at_unix_seconds = updated_at_unix_seconds;
        self.validate()?;

        Ok(self)
    }

    pub(crate) fn with_target_verified(
        mut self,
        target_reference: Option<&str>,
        updated_at_unix_seconds: i64,
    ) -> Result<Self, String> {
        let valid_source = self.phase == V7MigrationAdapterCheckpointPhase::RecoveryVerified
            || (!self.requires_recovery
                && self.phase == V7MigrationAdapterCheckpointPhase::Pending);
        if !valid_source {
            return Err(format!(
                "v7 migration adapter '{}' cannot verify a target before required recovery",
                self.adapter_id
            ));
        }
        if self.requires_recovery && target_reference.is_none() {
            return Err(format!(
                "v7 migration adapter '{}' requires a verified target identity",
                self.adapter_id
            ));
        }
        self.phase = V7MigrationAdapterCheckpointPhase::TargetVerified;
        self.target_reference = target_reference.map(str::to_owned);
        self.updated_at_unix_seconds = updated_at_unix_seconds;
        self.validate()?;

        Ok(self)
    }

    pub(crate) fn with_cutover(mut self, updated_at_unix_seconds: i64) -> Result<Self, String> {
        if self.phase != V7MigrationAdapterCheckpointPhase::TargetVerified {
            return Err(format!(
                "v7 migration adapter '{}' cannot cut over before target verification",
                self.adapter_id
            ));
        }
        self.phase = V7MigrationAdapterCheckpointPhase::Cutover;
        self.updated_at_unix_seconds = updated_at_unix_seconds;
        self.validate()?;

        Ok(self)
    }

    pub(crate) fn with_confirmed(mut self, updated_at_unix_seconds: i64) -> Result<Self, String> {
        if self.phase != V7MigrationAdapterCheckpointPhase::Cutover {
            return Err(format!(
                "v7 migration adapter '{}' cannot confirm before cutover",
                self.adapter_id
            ));
        }
        self.phase = V7MigrationAdapterCheckpointPhase::Confirmed;
        self.updated_at_unix_seconds = updated_at_unix_seconds;
        self.validate()?;

        Ok(self)
    }

    pub(crate) fn with_rolled_back(mut self, updated_at_unix_seconds: i64) -> Result<Self, String> {
        if matches!(
            self.phase,
            V7MigrationAdapterCheckpointPhase::Confirmed
                | V7MigrationAdapterCheckpointPhase::RolledBack
        ) {
            return Err(format!(
                "v7 migration adapter '{}' cannot roll back from a terminal phase",
                self.adapter_id
            ));
        }
        self.phase = V7MigrationAdapterCheckpointPhase::RolledBack;
        self.updated_at_unix_seconds = updated_at_unix_seconds;
        self.validate()?;

        Ok(self)
    }

    pub(crate) fn adapter_id(&self) -> &str {
        &self.adapter_id
    }

    pub(crate) fn adapter_kind(&self) -> &str {
        &self.adapter_kind
    }

    pub(crate) const fn requires_recovery(&self) -> bool {
        self.requires_recovery
    }

    pub(crate) fn recovery_reference(&self) -> Option<&str> {
        self.recovery_reference.as_deref()
    }

    pub(crate) fn recovery_artifact_sha256(&self) -> Option<&str> {
        self.recovery_artifact_sha256.as_deref()
    }

    pub(crate) const fn recovery_artifact_size_bytes(&self) -> Option<u64> {
        self.recovery_artifact_size_bytes
    }

    pub(crate) fn target_reference(&self) -> Option<&str> {
        self.target_reference.as_deref()
    }

    pub(super) const fn phase(&self) -> V7MigrationAdapterCheckpointPhase {
        self.phase
    }

    pub(super) fn can_advance_from(&self, previous: &Self) -> bool {
        self.phase == previous.phase
            || matches!(
                (previous.phase, self.phase),
                (
                    V7MigrationAdapterCheckpointPhase::Pending,
                    V7MigrationAdapterCheckpointPhase::RecoveryVerified
                ) | (
                    V7MigrationAdapterCheckpointPhase::Pending,
                    V7MigrationAdapterCheckpointPhase::TargetVerified
                ) | (
                    V7MigrationAdapterCheckpointPhase::RecoveryVerified,
                    V7MigrationAdapterCheckpointPhase::TargetVerified
                ) | (
                    V7MigrationAdapterCheckpointPhase::TargetVerified,
                    V7MigrationAdapterCheckpointPhase::Cutover
                ) | (
                    V7MigrationAdapterCheckpointPhase::Cutover,
                    V7MigrationAdapterCheckpointPhase::Confirmed
                )
            )
            || (previous.phase != V7MigrationAdapterCheckpointPhase::Confirmed
                && previous.phase != V7MigrationAdapterCheckpointPhase::RolledBack
                && self.phase == V7MigrationAdapterCheckpointPhase::RolledBack)
    }

    pub(super) fn has_same_identity(&self, other: &Self) -> bool {
        self.adapter_id == other.adapter_id
            && self.adapter_kind == other.adapter_kind
            && self.requires_recovery == other.requires_recovery
    }

    pub(super) fn preserves_evidence_from(&self, previous: &Self) -> bool {
        preserves_optional(
            previous.recovery_reference.as_ref(),
            self.recovery_reference.as_ref(),
        ) && preserves_optional(
            previous.recovery_artifact_sha256.as_ref(),
            self.recovery_artifact_sha256.as_ref(),
        ) && preserves_optional(
            previous.recovery_artifact_size_bytes.as_ref(),
            self.recovery_artifact_size_bytes.as_ref(),
        ) && preserves_optional(
            previous.target_reference.as_ref(),
            self.target_reference.as_ref(),
        )
    }

    pub(super) const fn updated_at_unix_seconds(&self) -> i64 {
        self.updated_at_unix_seconds
    }

    pub(super) fn validate(&self) -> Result<(), String> {
        let identity_valid = !self.adapter_id.is_empty()
            && !self.adapter_kind.is_empty()
            && !self.adapter_id.contains('\0')
            && !self.adapter_kind.contains('\0')
            && self.updated_at_unix_seconds >= 0;
        let recovery_complete = self
            .recovery_reference
            .as_ref()
            .is_some_and(|value| !value.is_empty() && !value.contains('\0'))
            && self
                .recovery_artifact_sha256
                .as_deref()
                .is_some_and(valid_sha256)
            && self
                .recovery_artifact_size_bytes
                .is_some_and(|size| size > 0 && size <= i64::MAX as u64);
        let recovery_absent = self.recovery_reference.is_none()
            && self.recovery_artifact_sha256.is_none()
            && self.recovery_artifact_size_bytes.is_none();
        let recovery_valid = if self.phase >= V7MigrationAdapterCheckpointPhase::RecoveryVerified
            && self.phase != V7MigrationAdapterCheckpointPhase::RolledBack
            && self.requires_recovery
        {
            recovery_complete
        } else {
            recovery_complete || recovery_absent
        };
        let target_valid = self
            .target_reference
            .as_ref()
            .is_none_or(|value| !value.is_empty() && !value.contains('\0'));
        let requires_target = self.requires_recovery
            && matches!(
                self.phase,
                V7MigrationAdapterCheckpointPhase::TargetVerified
                    | V7MigrationAdapterCheckpointPhase::Cutover
                    | V7MigrationAdapterCheckpointPhase::Confirmed
            );
        let target_complete = !requires_target || self.target_reference.is_some();
        if !identity_valid || !recovery_valid || !target_valid || !target_complete {
            return Err(format!(
                "v7 migration adapter '{}' has incomplete identity, recovery, target, or time evidence",
                self.adapter_id
            ));
        }

        Ok(())
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn preserves_optional<T: Eq>(previous: Option<&T>, next: Option<&T>) -> bool {
    previous.is_none_or(|previous| next == Some(previous))
}
