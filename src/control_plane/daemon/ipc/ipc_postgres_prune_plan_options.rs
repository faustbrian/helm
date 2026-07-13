/// Complete secret-free fields exposed for one exact PostgreSQL prune plan.
pub(crate) struct IpcPostgresPrunePlanOptions {
    pub(crate) project_id: String,
    pub(crate) service_id: String,
    pub(crate) logical_resource_id: String,
    pub(crate) shared_resource_id: String,
    pub(crate) compatibility_fingerprint: String,
    pub(crate) credential_id: String,
    pub(crate) recovery_point_id: String,
    pub(crate) confirmation_token: String,
}
