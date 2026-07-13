use super::VerifiedBackupEvidence;

/// Operator intent and backup evidence required to delete persistent data.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum PruneAuthorization {
    None,
    Explicit {
        backup: Option<VerifiedBackupEvidence>,
    },
}
