use crate::control_plane::shared_infrastructure::{MySqlFlavor, MySqlLogicalResourcePlan};
use crate::control_plane::state::CredentialRecord;
use std::path::Path;
use std::time::Duration;

/// Complete owned inputs for one explicit in-place development dump restore.
pub(crate) struct MySqlDumpRestoreOptions<'input> {
    pub(crate) flavor: MySqlFlavor,
    pub(crate) logical: &'input MySqlLogicalResourcePlan,
    pub(crate) credential: &'input CredentialRecord,
    pub(crate) administrator: &'input CredentialRecord,
    pub(crate) file: &'input Path,
    pub(crate) reset: bool,
    pub(crate) timeout: Duration,
}
