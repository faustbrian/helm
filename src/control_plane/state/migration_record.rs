use super::{MigrationPhase, MigrationRecordError, MigrationRecordOptions};

/// Durable, value-safe proof of one completed migration checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MigrationRecord {
    options: MigrationRecordOptions,
}

impl MigrationRecord {
    pub(crate) fn new(options: MigrationRecordOptions) -> Result<Self, MigrationRecordError> {
        if [
            &options.migration_id,
            &options.project_id,
            &options.source_revision,
            &options.target_revision,
            &options.source_compatibility_fingerprint,
            &options.target_compatibility_fingerprint,
        ]
        .into_iter()
        .any(|value| value.is_empty() || value.contains('\0'))
        {
            return Err(MigrationRecordError::MissingIdentity);
        }
        if options.updated_at_unix_seconds < 0 {
            return Err(MigrationRecordError::InvalidUpdateTime);
        }
        if options
            .backup_artifact_size_bytes
            .is_some_and(|size| size > i64::MAX as u64)
        {
            return Err(MigrationRecordError::InvalidBackupSize);
        }

        validate_phase_evidence(&options)?;

        Ok(Self { options })
    }

    pub(crate) fn migration_id(&self) -> &str {
        &self.options.migration_id
    }

    pub(crate) fn project_id(&self) -> &str {
        &self.options.project_id
    }

    pub(crate) fn source_revision(&self) -> &str {
        &self.options.source_revision
    }

    pub(crate) fn target_revision(&self) -> &str {
        &self.options.target_revision
    }

    pub(crate) fn source_compatibility_fingerprint(&self) -> &str {
        &self.options.source_compatibility_fingerprint
    }

    pub(crate) fn target_compatibility_fingerprint(&self) -> &str {
        &self.options.target_compatibility_fingerprint
    }

    pub(crate) const fn phase(&self) -> MigrationPhase {
        self.options.phase
    }

    pub(crate) fn backup_artifact_sha256(&self) -> Option<&str> {
        self.options.backup_artifact_sha256.as_deref()
    }

    pub(crate) fn backup_reference(&self) -> Option<&str> {
        self.options.backup_reference.as_deref()
    }

    pub(crate) const fn backup_artifact_size_bytes(&self) -> Option<u64> {
        self.options.backup_artifact_size_bytes
    }

    pub(crate) fn target_resource_id(&self) -> Option<&str> {
        self.options.target_resource_id.as_deref()
    }

    pub(crate) fn rollback_reference(&self) -> Option<&str> {
        self.options.rollback_reference.as_deref()
    }

    pub(crate) const fn updated_at_unix_seconds(&self) -> i64 {
        self.options.updated_at_unix_seconds
    }

    pub(crate) fn has_same_identity(&self, other: &Self) -> bool {
        self.migration_id() == other.migration_id()
            && self.project_id() == other.project_id()
            && self.source_revision() == other.source_revision()
            && self.target_revision() == other.target_revision()
            && self.source_compatibility_fingerprint() == other.source_compatibility_fingerprint()
            && self.target_compatibility_fingerprint() == other.target_compatibility_fingerprint()
    }

    pub(crate) fn preserves_evidence_from(&self, previous: &Self) -> bool {
        preserves_optional(previous.backup_reference(), self.backup_reference())
            && preserves_optional(
                previous.backup_artifact_sha256(),
                self.backup_artifact_sha256(),
            )
            && preserves_optional(
                previous.backup_artifact_size_bytes(),
                self.backup_artifact_size_bytes(),
            )
            && preserves_optional(previous.target_resource_id(), self.target_resource_id())
            && preserves_optional(previous.rollback_reference(), self.rollback_reference())
    }
}

fn preserves_optional<T: Eq>(previous: Option<T>, next: Option<T>) -> bool {
    previous.is_none_or(|previous| next.as_ref() == Some(&previous))
}

fn validate_phase_evidence(options: &MigrationRecordOptions) -> Result<(), MigrationRecordError> {
    let requires_backup = matches!(
        options.phase,
        MigrationPhase::BackupVerified
            | MigrationPhase::TargetProvisioned
            | MigrationPhase::DataRestored
            | MigrationPhase::TargetVerified
            | MigrationPhase::Cutover
            | MigrationPhase::Confirmed
    );
    if requires_backup
        && (options
            .backup_reference
            .as_ref()
            .is_none_or(String::is_empty)
            || options
                .backup_artifact_sha256
                .as_ref()
                .is_none_or(String::is_empty)
            || options.backup_artifact_size_bytes.is_none())
    {
        return Err(MigrationRecordError::MissingBackupEvidence {
            phase: options.phase,
        });
    }

    let requires_target = matches!(
        options.phase,
        MigrationPhase::TargetProvisioned
            | MigrationPhase::DataRestored
            | MigrationPhase::TargetVerified
            | MigrationPhase::Cutover
            | MigrationPhase::Confirmed
    );
    if requires_target
        && options
            .target_resource_id
            .as_ref()
            .is_none_or(String::is_empty)
    {
        return Err(MigrationRecordError::MissingTargetIdentity {
            phase: options.phase,
        });
    }

    if matches!(
        options.phase,
        MigrationPhase::Cutover | MigrationPhase::Confirmed
    ) && options
        .rollback_reference
        .as_ref()
        .is_none_or(String::is_empty)
    {
        return Err(MigrationRecordError::MissingRollbackMaterial {
            phase: options.phase,
        });
    }

    Ok(())
}
