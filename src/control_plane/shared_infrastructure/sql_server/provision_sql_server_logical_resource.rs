use super::{SqlServerLogicalResourcePlan, SqlServerSharedInstancePlan};
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, OwnedContainer,
    run_attached_command,
};
use std::collections::BTreeMap;
use std::time::Duration;

const PROVISIONING_TIMEOUT_SECONDS: u64 = 60;

/// Applies one SQL Server database/login plan through attached Engine exec.
pub(crate) async fn provision_sql_server_logical_resource(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    instance: &SqlServerSharedInstancePlan,
    plan: &SqlServerLogicalResourcePlan,
) -> Result<(), EngineError> {
    let request = CommandRequest::new(
        vec![
            instance.sqlcmd_path().to_owned(),
            "-b".to_owned(),
            "-C".to_owned(),
            "-S".to_owned(),
            "127.0.0.1".to_owned(),
            "-U".to_owned(),
            "sa".to_owned(),
            "-d".to_owned(),
            "master".to_owned(),
        ],
        BTreeMap::from([(
            "SQLCMDPASSWORD".to_owned(),
            instance.bootstrap_credential().secret().to_owned(),
        )]),
        None,
    )?;
    let options = AttachedCommandOptions::new(
        request,
        plan.stdin_sql().as_bytes().to_vec(),
        "provision SQL Server logical resource",
        Duration::from_secs(PROVISIONING_TIMEOUT_SECONDS),
    )?;

    run_attached_command(executor, container, &options).await
}
