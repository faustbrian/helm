use super::{MySqlLogicalResourcePlan, MySqlSharedInstancePlan};
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, OwnedContainer,
    run_attached_command,
};
use std::collections::BTreeMap;
use std::time::Duration;

const PROVISIONING_TIMEOUT_SECONDS: u64 = 30;
const CONNECTION_RETRY_ATTEMPTS: usize = 20;
const CONNECTION_RETRY_MILLISECONDS: u64 = 250;

/// Applies one MySQL-family schema/user plan through attached Engine exec.
pub(crate) async fn provision_mysql_logical_resource(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    instance: &MySqlSharedInstancePlan,
    plan: &MySqlLogicalResourcePlan,
) -> Result<(), EngineError> {
    let request = CommandRequest::new(
        vec![
            instance.flavor().client_executable().to_owned(),
            "--protocol=socket".to_owned(),
            "--user=root".to_owned(),
            "--batch".to_owned(),
            "--skip-column-names".to_owned(),
        ],
        BTreeMap::from([(
            "MYSQL_PWD".to_owned(),
            instance.bootstrap_credential().secret().to_owned(),
        )]),
        None,
    )?;
    let options = AttachedCommandOptions::new(
        request,
        plan.stdin_sql().as_bytes().to_vec(),
        format!(
            "provision {} logical resource",
            instance.flavor().implementation()
        ),
        Duration::from_secs(PROVISIONING_TIMEOUT_SECONDS),
    )?;

    for attempt in 1..=CONNECTION_RETRY_ATTEMPTS {
        match run_attached_command(executor, container, &options).await {
            Ok(()) => return Ok(()),
            Err(EngineError::ContainerExit { status_code: 1, .. })
                if attempt < CONNECTION_RETRY_ATTEMPTS =>
            {
                tokio::time::sleep(Duration::from_millis(CONNECTION_RETRY_MILLISECONDS)).await;
            }
            Err(error) => return Err(error),
        }
    }

    unreachable!("the final MySQL-family provisioning attempt always returns")
}
