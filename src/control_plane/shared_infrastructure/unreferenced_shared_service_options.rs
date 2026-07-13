use crate::control_plane::state::{LogicalResourceRecord, ResourceRecord};

/// Complete durable evidence for idling unreferenced shared processes.
pub(crate) struct UnreferencedSharedServiceOptions<'operation> {
    pub(crate) resources: &'operation [ResourceRecord],
    pub(crate) logical_resources: &'operation [LogicalResourceRecord],
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
}
