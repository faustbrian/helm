use crate::control_plane::state::{LogicalResourceRecord, ResourceRecord};

/// Typed v8 state produced by normal reconciliation for one recreated service.
pub(crate) enum V7RecreatedServiceTarget {
    Workload(ResourceRecord),
    Logical(LogicalResourceRecord),
    Ephemeral,
}
