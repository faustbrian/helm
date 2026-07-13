use super::RecoveryPointRecordOptions;

/// Durable immutable ownership and integrity evidence for one recovery point.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecoveryPointRecord {
    options: RecoveryPointRecordOptions,
}

impl RecoveryPointRecord {
    pub(crate) fn new(options: RecoveryPointRecordOptions) -> Result<Self, String> {
        let valid = [
            options.recovery_point_id.as_str(),
            options.project_id.as_str(),
            options.service_id.as_str(),
            options.logical_resource_id.as_str(),
            options.resource_kind.as_str(),
            options.compatibility_fingerprint.as_str(),
            options.reference.as_str(),
        ]
        .into_iter()
        .all(|value| !value.is_empty())
            && options.artifact_sha256.len() == 64
            && options
                .artifact_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            && options.artifact_size_bytes > 0
            && options.created_at_unix_seconds >= 0
            && options.verified_at_unix_seconds >= options.created_at_unix_seconds;
        if !valid {
            return Err(
                "recovery point requires complete immutable ownership and integrity evidence"
                    .to_owned(),
            );
        }

        Ok(Self { options })
    }

    pub(crate) fn recovery_point_id(&self) -> &str {
        &self.options.recovery_point_id
    }

    pub(crate) fn project_id(&self) -> &str {
        &self.options.project_id
    }

    pub(crate) fn service_id(&self) -> &str {
        &self.options.service_id
    }

    pub(crate) fn logical_resource_id(&self) -> &str {
        &self.options.logical_resource_id
    }

    pub(crate) fn resource_kind(&self) -> &str {
        &self.options.resource_kind
    }

    pub(crate) fn compatibility_fingerprint(&self) -> &str {
        &self.options.compatibility_fingerprint
    }

    pub(crate) fn reference(&self) -> &str {
        &self.options.reference
    }

    pub(crate) fn artifact_sha256(&self) -> &str {
        &self.options.artifact_sha256
    }

    pub(crate) const fn artifact_size_bytes(&self) -> u64 {
        self.options.artifact_size_bytes
    }

    pub(crate) const fn created_at_unix_seconds(&self) -> i64 {
        self.options.created_at_unix_seconds
    }

    pub(crate) const fn verified_at_unix_seconds(&self) -> i64 {
        self.options.verified_at_unix_seconds
    }
}
