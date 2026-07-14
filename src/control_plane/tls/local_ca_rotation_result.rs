use super::LocalCaIdentity;

/// Exact identities replaced by one completed Stackctl CA rotation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LocalCaRotationResult {
    previous_identity: LocalCaIdentity,
    current_identity: LocalCaIdentity,
}

impl LocalCaRotationResult {
    pub(super) const fn new(
        previous_identity: LocalCaIdentity,
        current_identity: LocalCaIdentity,
    ) -> Self {
        Self {
            previous_identity,
            current_identity,
        }
    }

    pub(crate) const fn previous_identity(&self) -> &LocalCaIdentity {
        &self.previous_identity
    }

    pub(crate) const fn current_identity(&self) -> &LocalCaIdentity {
        &self.current_identity
    }
}
