#[cfg(test)]
use crate::control_plane::retention::DataLifecycleStrategy;

/// Complete secret-free fields exposed for one exact logical prune plan.
#[cfg(test)]
pub(crate) struct IpcPostgresPrunePlanOptions {
    pub(crate) strategy: DataLifecycleStrategy,
    pub(crate) project_id: String,
    pub(crate) service_id: String,
    pub(crate) logical_resource_id: String,
    pub(crate) shared_resource_id: String,
    pub(crate) compatibility_fingerprint: String,
    pub(crate) credential_id: String,
    pub(crate) recovery_point_id: String,
    pub(crate) confirmation_token: String,
}
