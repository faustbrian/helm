use super::MigrationPhase;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// A migration checkpoint that lacks safe identity or phase evidence.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum MigrationRecordError {
    MissingIdentity,
    InvalidUpdateTime,
    InvalidBackupSize,
    MissingBackupEvidence { phase: MigrationPhase },
    MissingTargetIdentity { phase: MigrationPhase },
    MissingRollbackMaterial { phase: MigrationPhase },
}

impl Display for MigrationRecordError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingIdentity => {
                formatter.write_str("migration identity fields must not be empty")
            }
            Self::InvalidUpdateTime => {
                formatter.write_str("migration update time must not be negative")
            }
            Self::InvalidBackupSize => {
                formatter.write_str("migration backup size exceeds durable storage limits")
            }
            Self::MissingBackupEvidence { phase } => write!(
                formatter,
                "migration phase '{phase}' requires verified backup evidence"
            ),
            Self::MissingTargetIdentity { phase } => write!(
                formatter,
                "migration phase '{phase}' requires a target resource identity"
            ),
            Self::MissingRollbackMaterial { phase } => write!(
                formatter,
                "migration phase '{phase}' requires retained rollback material"
            ),
        }
    }
}

impl Error for MigrationRecordError {}
