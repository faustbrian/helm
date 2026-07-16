use super::ManagedResourceMetadata;

/// The proven relationship between an observed object and this installation.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum ObservedResourceOwnership {
    Owned(Box<ManagedResourceMetadata>),
    Unmanaged,
    ForeignInstallation { installation_id: String },
    UnsupportedSchema { found: u32, supported: u32 },
    Malformed { detail: String },
}
