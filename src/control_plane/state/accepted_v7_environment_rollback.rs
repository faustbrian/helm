use std::path::{Path, PathBuf};

/// Secret-free durable pointer to protected exact legacy `.env` bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AcceptedV7EnvironmentRollback {
    reference: PathBuf,
    artifact_sha256: String,
    artifact_size_bytes: u64,
}

impl AcceptedV7EnvironmentRollback {
    pub(crate) fn new(
        reference: PathBuf,
        artifact_sha256: String,
        artifact_size_bytes: u64,
    ) -> Result<Self, String> {
        let valid_checksum = artifact_sha256.len() == 64
            && artifact_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        if !reference.is_absolute()
            || !valid_checksum
            || artifact_size_bytes == 0
            || artifact_size_bytes > i64::MAX.unsigned_abs()
        {
            return Err(
                "accepted v7 environment rollback requires an absolute reference, lowercase SHA-256, and positive size"
                    .to_owned(),
            );
        }

        Ok(Self {
            reference,
            artifact_sha256,
            artifact_size_bytes,
        })
    }

    pub(crate) fn reference(&self) -> &Path {
        &self.reference
    }

    pub(crate) fn artifact_sha256(&self) -> &str {
        &self.artifact_sha256
    }

    pub(crate) const fn artifact_size_bytes(&self) -> u64 {
        self.artifact_size_bytes
    }
}
