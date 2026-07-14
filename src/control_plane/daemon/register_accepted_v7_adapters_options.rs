use super::{
    RegisterAcceptedV7LogicalDataAdaptersOptions, RegisterAcceptedV7NamedVolumeAdaptersOptions,
    RegisterAcceptedV7ProjectWideAdaptersOptions,
};
use crate::control_plane::state::{LogicalResourceRecord, ResourceRecord};

/// Complete strategy contexts for one immutable accepted-v7 adapter set.
pub(crate) struct RegisterAcceptedV7AdaptersOptions<'operation, E> {
    pub(crate) resources: &'operation [ResourceRecord],
    pub(crate) logical_resources: &'operation [LogicalResourceRecord],
    pub(crate) logical: RegisterAcceptedV7LogicalDataAdaptersOptions<'operation, E>,
    pub(crate) named_volumes: RegisterAcceptedV7NamedVolumeAdaptersOptions<'operation, E>,
    pub(crate) project_wide: RegisterAcceptedV7ProjectWideAdaptersOptions<'operation>,
}
