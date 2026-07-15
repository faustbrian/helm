use super::RedisSharedInstancePlan;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, OwnedContainer,
};
use crate::control_plane::shared_infrastructure::run_shared_service_readiness_probe;
use std::collections::BTreeMap;
use std::time::Duration;

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

    run_shared_service_readiness_probe(executor, container, &options).await
}
