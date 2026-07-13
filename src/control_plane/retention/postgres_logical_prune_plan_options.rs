use crate::control_plane::state::{CredentialRecord, LogicalResourceRecord, RecoveryPointRecord};

/// Complete immutable state used to authorize one PostgreSQL logical prune.
pub(crate) struct PostgresLogicalPrunePlanOptions<'state> {
    pub(crate) installation_id: &'state str,
    pub(crate) project_id: &'state str,
    pub(crate) service_id: &'state str,
    pub(crate) recovery_point_id: &'state str,
    pub(crate) project_registered: bool,
    pub(crate) logical_resources: &'state [LogicalResourceRecord],
    pub(crate) credentials: &'state [CredentialRecord],
    pub(crate) recovery_points: &'state [RecoveryPointRecord],
}
