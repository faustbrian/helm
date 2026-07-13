use crate::control_plane::ProjectIdentity;
use std::path::PathBuf;

/// Complete immutable inputs for one dedicated project application container.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ApplicationContainerPlanOptions {
    pub(crate) project: ProjectIdentity,
    pub(crate) image_digest: String,
    pub(crate) source_path: PathBuf,
    pub(crate) network_name: String,
    pub(crate) internal_http_port: u16,
}
