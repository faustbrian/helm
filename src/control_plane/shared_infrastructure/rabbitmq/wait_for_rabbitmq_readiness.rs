use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, OwnedContainer,
};
use crate::control_plane::shared_infrastructure::run_shared_service_readiness_probe;
use std::collections::BTreeMap;
use std::time::Duration;

const READINESS_PROBE_TIMEOUT_SECONDS: u64 = 5;

/// Waits until the local RabbitMQ node accepts broker operations.
pub(crate) async fn wait_for_rabbitmq_readiness(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
) -> Result<(), EngineError> {
    let request = CommandRequest::new(
        vec![
            "rabbitmq-diagnostics".to_owned(),
            "-q".to_owned(),
            "check_running".to_owned(),
        ],
        BTreeMap::new(),
        None,
    )?;
    let options = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "wait for RabbitMQ readiness",
        Duration::from_secs(READINESS_PROBE_TIMEOUT_SECONDS),
    )?;

    run_shared_service_readiness_probe(executor, container, &options).await
}
