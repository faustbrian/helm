use super::LocalCaIdentity;

/// Read-only trust state for the exact persisted singleton CA.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CurrentCaTrustStatus {
    Absent,
    Untrusted(LocalCaIdentity),
    Trusted(LocalCaIdentity),
}
