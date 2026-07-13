/// Inputs required to materialize one shared stateless Gotenberg instance.
pub(crate) struct GotenbergSharedInstancePlanOptions {
    pub(crate) installation_id: String,
    pub(crate) network_name: String,
    pub(crate) schema_version: u32,
    pub(crate) desired_revision: String,
}
