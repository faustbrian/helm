use super::MySqlSharedInstancePlan;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, OwnedContainer,
};
use crate::control_plane::shared_infrastructure::run_shared_service_readiness_probe;
use crate::control_plane::state::CredentialLifecycle;
use std::collections::BTreeMap;
use std::time::Duration;

const READINESS_PROBE_TIMEOUT_SECONDS: u64 = 5;

/// Waits for one MySQL-compatible server through its root identity.
pub(super) async fn wait_for_mysql_readiness(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    instance: &MySqlSharedInstancePlan,
) -> Result<(), EngineError> {
    let administrator = instance.bootstrap_credential();
    if administrator.username() != "root"
        || administrator.secret().is_empty()
        || administrator.lifecycle() != CredentialLifecycle::Active
    {
        return Err(EngineError::InvalidRequest {
            detail: format!(
                "{} readiness requires the active root administrator",
                instance.flavor().implementation()
            ),
        });
    }
    let request = CommandRequest::new(
        vec![
            instance.flavor().client_executable().to_owned(),
            "--protocol=socket".to_owned(),
            "--user=root".to_owned(),
            "--batch".to_owned(),
            "--skip-column-names".to_owned(),
            "--execute=SELECT 1".to_owned(),
        ],
        BTreeMap::from([("MYSQL_PWD".to_owned(), administrator.secret().to_owned())]),
        None,
    )?;
    let options = AttachedCommandOptions::new(
        request,
        Vec::new(),
        format!("wait for {} readiness", instance.flavor().implementation()),
        Duration::from_secs(READINESS_PROBE_TIMEOUT_SECONDS),
    )?;

    run_shared_service_readiness_probe(executor, container, &options).await
}
