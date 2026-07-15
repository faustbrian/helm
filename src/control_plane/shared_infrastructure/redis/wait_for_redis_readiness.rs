use super::RedisSharedInstancePlan;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, OwnedContainer,
    run_attached_command,
};
use std::collections::BTreeMap;
use std::time::Duration;

const READINESS_ATTEMPTS: usize = 30;
const READINESS_RETRY_MILLISECONDS: u64 = 200;
const READINESS_PROBE_TIMEOUT_SECONDS: u64 = 5;

/// Waits for one Redis-compatible process through its bootstrap identity.
pub(super) async fn wait_for_redis_readiness(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    instance: &RedisSharedInstancePlan,
) -> Result<(), EngineError> {
    let flavor = instance.flavor();
    let request = CommandRequest::new(
        vec![
            flavor.client_executable().to_owned(),
            "-e".to_owned(),
            "--user".to_owned(),
            instance.bootstrap_credential().username().to_owned(),
            "PING".to_owned(),
        ],
        BTreeMap::from([(
            flavor.client_auth_environment_key().to_owned(),
            instance.bootstrap_credential().secret().to_owned(),
        )]),
        None,
    )?;
    let options = AttachedCommandOptions::new(
        request,
        Vec::new(),
        format!("wait for {} readiness", flavor.implementation()),
        Duration::from_secs(READINESS_PROBE_TIMEOUT_SECONDS),
    )?;

    for attempt in 1..=READINESS_ATTEMPTS {
        match run_attached_command(executor, container, &options).await {
            Ok(()) => return Ok(()),
            Err(EngineError::ContainerExit { .. } | EngineError::Backend { .. })
                if attempt < READINESS_ATTEMPTS =>
            {
                tokio::time::sleep(Duration::from_millis(READINESS_RETRY_MILLISECONDS)).await;
            }
            Err(error) => return Err(error),
        }
    }

    unreachable!("the final Redis readiness attempt always returns")
}
