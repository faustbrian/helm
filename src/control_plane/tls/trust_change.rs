/// Observable result of reconciling one Stackctl CA trust entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrustChange {
    Unchanged,
    Installed,
    Removed,
}
