use crate::control_plane::{ProjectIdentity, ServiceIdentity};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Complete desired inputs for one long-running project process.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct ProjectProcessPlanOptions {
    pub(crate) project: ProjectIdentity,
    pub(crate) service: ServiceIdentity,
    pub(crate) image_digest: String,
    pub(crate) source_path: PathBuf,
    pub(crate) network_name: String,
    pub(crate) command: Vec<String>,
    pub(crate) environment: BTreeMap<String, String>,
}
