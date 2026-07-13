use super::{BackupVerificationError, RestoreTargetError};
use std::error::Error;
use std::fmt::{Display, Formatter};

/// A restore rejected before, during, or after isolated staging.
#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum RestoreError {
    InvalidRestoreId,
    Backup(BackupVerificationError),
    BackupResourceMismatch,
    IncompleteArtifact,
    StreamChecksumMismatch,
    StreamSizeMismatch,
    Target(RestoreTargetError),
    Rollback {
        primary: Box<Self>,
        rollback: RestoreTargetError,
    },
}

impl Display for RestoreError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRestoreId => formatter.write_str("restore id must be non-empty and valid"),
            Self::Backup(error) => Display::fmt(error, formatter),
            Self::BackupResourceMismatch => {
                formatter.write_str("backup evidence does not match the restore resource")
            }
            Self::IncompleteArtifact => {
                formatter.write_str("restore target did not consume the complete backup artifact")
            }
            Self::StreamChecksumMismatch => {
                formatter.write_str("staged backup checksum changed after verification")
            }
            Self::StreamSizeMismatch => {
                formatter.write_str("staged backup size changed after verification")
            }
            Self::Target(error) => Display::fmt(error, formatter),
            Self::Rollback { primary, rollback } => {
                write!(formatter, "{primary}; rollback also failed: {rollback}")
            }
        }
    }
}

impl Error for RestoreError {}

impl From<BackupVerificationError> for RestoreError {
    fn from(error: BackupVerificationError) -> Self {
        Self::Backup(error)
    }
}
