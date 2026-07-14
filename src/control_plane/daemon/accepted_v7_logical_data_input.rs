use super::V7LogicalDataCredential;
use crate::control_plane::migration::V7LogicalDataMigrationSource;

/// Exact accepted source and secret-bearing driver credential for composition.
#[derive(Debug)]
pub(crate) struct AcceptedV7LogicalDataInput {
    source: V7LogicalDataMigrationSource,
    credential: V7LogicalDataCredential,
}

impl AcceptedV7LogicalDataInput {
    pub(super) const fn new(
        source: V7LogicalDataMigrationSource,
        credential: V7LogicalDataCredential,
    ) -> Self {
        Self { source, credential }
    }

    pub(crate) const fn source(&self) -> &V7LogicalDataMigrationSource {
        &self.source
    }

    pub(crate) const fn credential(&self) -> &V7LogicalDataCredential {
        &self.credential
    }
}
