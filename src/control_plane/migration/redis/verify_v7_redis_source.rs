use super::V7RedisCredential;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandRequest, V7ContainerCommandExecutor, V7ContainerCommandTarget,
    run_v7_attached_command_capture,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::shared_infrastructure::RedisFlavor;
use std::collections::BTreeMap;
use std::time::Duration;

const VERIFY_SCRIPT: &str = "return {redis.call('ACL', 'WHOAMI'), redis.call('PING')}";

pub(super) async fn verify_v7_redis_source(
    executor: &(impl V7ContainerCommandExecutor + Sync),
    target: &V7ContainerCommandTarget,
    flavor: RedisFlavor,
    credential: &V7RedisCredential,
    timeout: Duration,
) -> Result<(), MigrationOperationError> {
    let request = verification_request(
        flavor,
        credential.username(),
        credential.password(),
        credential.database(),
    )?;
    let command = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "verify retained v7 Redis-compatible source",
        timeout,
    )
    .map_err(|error| operation_error("v7 Redis-compatible verification is invalid", error))?;
    let output = run_v7_attached_command_capture(executor, target, &command)
        .await
        .map_err(|error| operation_error("verify retained v7 Redis-compatible source", error))?;
    let expected = format!("{}\nPONG\n", credential.username());
    if output != expected.as_bytes() {
        return Err(MigrationOperationError::new(
            "retained v7 Redis-compatible source returned unexpected identity evidence",
        ));
    }
    Ok(())
}

pub(super) fn verification_request(
    flavor: RedisFlavor,
    username: &str,
    password: &str,
    database: u32,
) -> Result<CommandRequest, MigrationOperationError> {
    let mut environment = BTreeMap::new();
    if !password.is_empty() {
        environment.insert(
            flavor.client_auth_environment_key().to_owned(),
            password.to_owned(),
        );
    }
    CommandRequest::new(
        vec![
            flavor.client_executable().to_owned(),
            "--raw".to_owned(),
            "--user".to_owned(),
            username.to_owned(),
            "-n".to_owned(),
            database.to_string(),
            "EVAL".to_owned(),
            VERIFY_SCRIPT.to_owned(),
            "0".to_owned(),
        ],
        environment,
        None,
    )
    .map_err(|error| operation_error("Redis-compatible verification request is invalid", error))
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
