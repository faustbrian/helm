use super::CredentialLifecycle;

/// Complete durable fields for one project-scoped managed credential.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct CredentialRecordOptions {
    pub(crate) credential_id: String,
    pub(crate) project_id: Option<String>,
    pub(crate) service_id: String,
    pub(crate) username: String,
    pub(crate) secret: String,
    pub(crate) lifecycle: CredentialLifecycle,
}
