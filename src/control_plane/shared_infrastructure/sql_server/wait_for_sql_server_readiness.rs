use super::SqlServerSharedInstancePlan;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, OwnedContainer,
};
use crate::control_plane::shared_infrastructure::run_shared_service_readiness_probe;
use crate::control_plane::state::CredentialLifecycle;
use std::collections::BTreeMap;
use std::time::Duration;

const READINESS_PROBE_TIMEOUT_SECONDS: u64 = 5;

/// Waits for SQL Server through its managed administrator identity.
pub(super) async fn wait_for_sql_server_readiness(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    instance: &SqlServerSharedInstancePlan,
) -> Result<(), EngineError> {
    let administrator = instance.bootstrap_credential();
    if administrator.username() != "sa"
        || administrator.secret().is_empty()
        || administrator.lifecycle() != CredentialLifecycle::Active
    {
        return Err(EngineError::InvalidRequest {
            detail: "SQL Server readiness requires the active sa administrator".to_owned(),
        });
    }
    let request = CommandRequest::new(
        vec![
            instance.sqlcmd_path().to_owned(),
            "-b".to_owned(),
            "-C".to_owned(),
            "-S".to_owned(),
            "127.0.0.1".to_owned(),
            "-l".to_owned(),
            "1".to_owned(),
            "-U".to_owned(),
            "sa".to_owned(),
            "-Q".to_owned(),
            "SET NOCOUNT ON; SELECT 1".to_owned(),
        ],
        BTreeMap::from([(
            "SQLCMDPASSWORD".to_owned(),
            administrator.secret().to_owned(),
        )]),
        None,
    )?;
    let options = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "wait for SQL Server readiness",
        Duration::from_secs(READINESS_PROBE_TIMEOUT_SECONDS),
    )?;

    run_shared_service_readiness_probe(executor, container, &options).await
}
