/// Operator intent and backup evidence required to delete persistent data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum PruneAuthorization {
    None,
    Explicit { verified_backup: bool },
}
