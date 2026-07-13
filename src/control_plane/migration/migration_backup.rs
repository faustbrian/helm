use super::MigrationOperationError;

/// Durable reference and integrity proof returned by a backup adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MigrationBackup {
    reference: String,
    artifact_sha256: String,
    artifact_size_bytes: u64,
}

impl MigrationBackup {
    pub(crate) fn new(
        reference: impl Into<String>,
        artifact_sha256: impl Into<String>,
        artifact_size_bytes: u64,
    ) -> Result<Self, MigrationOperationError> {
        let reference = reference.into();
        let artifact_sha256 = artifact_sha256.into();
        if reference.is_empty() || artifact_sha256.is_empty() || artifact_size_bytes == 0 {
            return Err(MigrationOperationError::new(
                "backup operation returned incomplete verified evidence",
            ));
        }

        Ok(Self {
            reference,
            artifact_sha256,
            artifact_size_bytes,
        })
    }

    pub(crate) fn reference(&self) -> &str {
        &self.reference
    }

    pub(crate) fn artifact_sha256(&self) -> &str {
        &self.artifact_sha256
    }

    pub(crate) const fn artifact_size_bytes(&self) -> u64 {
        self.artifact_size_bytes
    }
}
