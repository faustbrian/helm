use super::ProjectCommand;
use crate::control_plane::ProjectIdentity;
use std::collections::BTreeMap;
use std::time::Duration;

/// Complete bounded inputs for one in-container project command.
pub(crate) struct ProjectCommandPlanOptions {
    pub(crate) project: ProjectIdentity,
    pub(crate) command: ProjectCommand,
    pub(crate) environment: BTreeMap<String, String>,
    pub(crate) input: Vec<u8>,
    pub(crate) timeout: Duration,
    pub(crate) browser_session: bool,
}
