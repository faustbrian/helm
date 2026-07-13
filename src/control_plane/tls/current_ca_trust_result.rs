use super::{LocalCaIdentity, TrustChange};

/// Exact CA identity and observable result of one trust operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CurrentCaTrustResult {
    identity: LocalCaIdentity,
    change: TrustChange,
}

impl CurrentCaTrustResult {
    pub(super) const fn new(identity: LocalCaIdentity, change: TrustChange) -> Self {
        Self { identity, change }
    }

    pub(crate) const fn identity(&self) -> &LocalCaIdentity {
        &self.identity
    }

    pub(crate) const fn change(&self) -> TrustChange {
        self.change
    }
}
