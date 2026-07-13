use std::path::PathBuf;

/// Inputs required to materialize one shared RabbitMQ instance.
pub(crate) struct RabbitMqSharedInstancePlanOptions {
    pub(crate) installation_id: String,
    pub(crate) network_name: String,
    pub(crate) schema_version: u32,
    pub(crate) desired_revision: String,
    pub(crate) definitions_directory: PathBuf,
}
