use super::RedisSharedInstancePlan;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, OwnedContainer,
    run_attached_command,
};
use std::collections::BTreeMap;
use std::time::Duration;

const ACL_RELOAD_TIMEOUT_SECONDS: u64 = 15;

/// Atomically reloads the complete mounted ACL snapshot in one owned process.
pub(crate) async fn reload_redis_acl(
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
            "ACL".to_owned(),
            "LOAD".to_owned(),
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
        format!("reload {} ACL snapshot", flavor.implementation()),
        Duration::from_secs(ACL_RELOAD_TIMEOUT_SECONDS),
    )?;

    run_attached_command(executor, container, &options).await
}
