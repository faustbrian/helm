use crate::control_plane::shared_infrastructure::CredentialSecret;

/// Inputs required to materialize one shared MongoDB instance.
pub(crate) struct MongoDbSharedInstancePlanOptions {
    pub(crate) installation_id: String,
    pub(crate) network_name: String,
    pub(crate) schema_version: u32,
    pub(crate) desired_revision: String,
    pub(crate) bootstrap_secret: CredentialSecret,
}
