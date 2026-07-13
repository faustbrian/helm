use std::error::Error;
use std::fmt::{Display, Formatter};

/// A backup artifact that cannot provide trustworthy deletion evidence.
#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum BackupVerificationError {
    EmptyArtifact,
    InvalidCreationTime,
    InvalidVerificationTime,
    VerificationPredatesCreation,
    ChecksumMismatch,
    SizeMismatch,
    InvalidManifest { detail: String },
    Storage { detail: String },
}

impl Display for BackupVerificationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::EmptyArtifact => "backup artifact must not be empty",
            Self::InvalidCreationTime => "backup artifact creation time must not be negative",
            Self::InvalidVerificationTime => "backup verification time must not be negative",
            Self::VerificationPredatesCreation => {
                "backup verification time predates artifact creation"
            }
            Self::ChecksumMismatch => "backup artifact checksum does not match its manifest",
            Self::SizeMismatch => "backup artifact size does not match its manifest",
            Self::InvalidManifest { detail } | Self::Storage { detail } => detail,
        })
    }
}

impl Error for BackupVerificationError {}
