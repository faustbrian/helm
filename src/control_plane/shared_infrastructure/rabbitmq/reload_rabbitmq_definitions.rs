use super::RabbitMqSharedInstancePlan;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, OwnedContainer,
    run_attached_command,
};
use std::collections::BTreeMap;
use std::time::Duration;

const DEFINITIONS_IMPORT_TIMEOUT_SECONDS: u64 = 30;

/// Imports the complete mounted core definitions without restarting the broker.
pub(crate) async fn reload_rabbitmq_definitions(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    instance: &RabbitMqSharedInstancePlan,
) -> Result<(), EngineError> {
    let request = CommandRequest::new(
        vec![
            "rabbitmqctl".to_owned(),
            "import_definitions".to_owned(),
            instance.definitions_file().to_owned(),
        ],
        BTreeMap::new(),
        None,
    )?;
    let options = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "reload RabbitMQ core definitions",
        Duration::from_secs(DEFINITIONS_IMPORT_TIMEOUT_SECONDS),
    )?;

    run_attached_command(executor, container, &options).await
}
