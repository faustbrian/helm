use super::{LogicalResourceRecord, ResourceRecord};
use std::path::PathBuf;

/// Complete target state for one explicit project adoption transaction.
pub(crate) struct ProjectAdoptionPlanOptions {
    pub(crate) canonical_path: PathBuf,
    pub(crate) project_id: String,
    pub(crate) resources: Vec<ResourceRecord>,
    pub(crate) logical_resources: Vec<LogicalResourceRecord>,
    pub(crate) credential_ids: Vec<String>,
    pub(crate) environment_revision: String,
}
