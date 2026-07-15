use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, OwnedContainer,
};
use crate::control_plane::shared_infrastructure::run_shared_service_readiness_probe;
use crate::control_plane::state::{CredentialLifecycle, CredentialRecord};
use std::collections::BTreeMap;
use std::time::Duration;

const MONGODB_BOOTSTRAP_USERNAME: &str = "stackctl_admin";
const READINESS_PROBE_TIMEOUT_SECONDS: u64 = 5;

/// Waits for MongoDB through its managed root identity.
pub(crate) async fn wait_for_mongodb_readiness(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    administrator: &CredentialRecord,
) -> Result<(), EngineError> {
    if administrator.username() != MONGODB_BOOTSTRAP_USERNAME
        || administrator.secret().is_empty()
        || administrator.lifecycle() != CredentialLifecycle::Active
    {
        return Err(EngineError::InvalidRequest {
            detail: "MongoDB readiness requires the active bootstrap administrator".to_owned(),
        });
    }
    let username = serde_json::to_string(administrator.username()).map_err(|error| {
        EngineError::InvalidRequest {
            detail: format!("failed to encode MongoDB readiness username: {error}"),
        }
    })?;
    let password = serde_json::to_string(administrator.secret()).map_err(|error| {
        EngineError::InvalidRequest {
            detail: format!("failed to encode MongoDB readiness secret: {error}"),
        }
    })?;
    let script = format!(
        "try {{\n\
         const admin = connect(\"mongodb://127.0.0.1:27017/admin\");\n\
         if (!admin.auth({username}, {password})) {{ quit(1); }}\n\
         if (admin.runCommand({{ ping: 1 }}).ok !== 1) {{ quit(1); }}\n\
         quit(0);\n\
         }} catch (error) {{ quit(1); }}\n"
    );
    let request = CommandRequest::new(
        vec![
            "mongosh".to_owned(),
            "--quiet".to_owned(),
            "--nodb".to_owned(),
        ],
        BTreeMap::new(),
        None,
    )?;
    let options = AttachedCommandOptions::new(
        request,
        script.into_bytes(),
        "wait for MongoDB readiness",
        Duration::from_secs(READINESS_PROBE_TIMEOUT_SECONDS),
    )?;

    run_shared_service_readiness_probe(executor, container, &options).await
}
