use super::VerifiedBackupEvidence;

/// Operator intent and backup evidence required to delete persistent data.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum PruneAuthorization {
    None,
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "explicit deletion authorization contract")
    )]
    Explicit {
        backup: Option<VerifiedBackupEvidence>,
    },
}
