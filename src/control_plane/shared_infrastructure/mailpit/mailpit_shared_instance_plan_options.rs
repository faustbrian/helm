use std::path::PathBuf;

/// Inputs required to materialize one attributed shared Mailpit instance.
pub(crate) struct MailpitSharedInstancePlanOptions {
    pub(crate) installation_id: String,
    pub(crate) network_name: String,
    pub(crate) schema_version: u32,
    pub(crate) desired_revision: String,
    pub(crate) authentication_directory: PathBuf,
    pub(crate) authentication_revision: String,
}
