use super::{RuntimeEnvironment, RuntimeImageBuildPlan};
use crate::control_plane::ProjectIdentity;
use crate::control_plane::engine::ManagedResourceMetadata;
use std::path::PathBuf;

/// Complete inputs for one project application reconciliation.
pub(crate) struct ProjectRuntimeReconcileOptions<'operation> {
    pub(crate) runtime_image: &'operation RuntimeImageBuildPlan,
    pub(crate) project: ProjectIdentity,
    pub(crate) source_path: PathBuf,
    pub(crate) network_name: String,
    pub(crate) internal_http_port: u16,
    pub(crate) application_metadata: ManagedResourceMetadata,
    pub(crate) platform: String,
    pub(crate) command: Vec<String>,
    pub(crate) environment: RuntimeEnvironment,
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
}
