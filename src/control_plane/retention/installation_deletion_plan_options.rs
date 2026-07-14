use crate::control_plane::state::{
    CredentialRecord, LogicalResourceRecord, RecoveryPointRecord, ResourceRecord,
};

/// Complete state required to prove an installation can be deleted safely.
pub(crate) struct InstallationDeletionPlanOptions<'state> {
    pub(crate) installation_id: &'state str,
    pub(crate) logical_resources: &'state [LogicalResourceRecord],
    pub(crate) resources: &'state [ResourceRecord],
    pub(crate) credentials: &'state [CredentialRecord],
    pub(crate) recovery_points: &'state [RecoveryPointRecord],
}
