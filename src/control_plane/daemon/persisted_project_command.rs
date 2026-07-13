use super::QueuedProjectCommand;
use crate::control_plane::ProjectIdentity;
use crate::control_plane::workload::{
    ProjectCommand, ProjectCommandPlan, ProjectCommandPlanOptions,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

const MAX_TIMEOUT_MILLISECONDS: u64 = 3_600_000;

/// Exact validated command payload retained across daemon restarts.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PersistedProjectCommand {
    project_id: String,
    service_id: String,
    arguments: Vec<String>,
    environment: BTreeMap<String, String>,
    input: Vec<u8>,
    timeout_milliseconds: u64,
    #[serde(default)]
    browser_session: bool,
}

impl PersistedProjectCommand {
    pub(crate) fn from_queued(operation: &QueuedProjectCommand) -> Result<Self, String> {
        let timeout_milliseconds = u64::try_from(operation.plan().timeout().as_millis())
            .map_err(|_| "project command timeout exceeds durable range".to_owned())?;

        Ok(Self {
            project_id: operation.plan().project_id().to_owned(),
            service_id: operation.service_id().to_owned(),
            arguments: operation.plan().arguments().to_vec(),
            environment: operation.plan().environment().clone(),
            input: operation.plan().input().to_vec(),
            timeout_milliseconds,
            browser_session: operation.plan().browser_session(),
        })
    }

    pub(crate) fn into_queued(self, operation_id: String) -> Result<QueuedProjectCommand, String> {
        if self.timeout_milliseconds == 0 || self.timeout_milliseconds > MAX_TIMEOUT_MILLISECONDS {
            return Err(
                "persisted project command timeout is outside the supported range".to_owned(),
            );
        }
        let project = ProjectIdentity::resolve(Some(&self.project_id), Path::new("/"))
            .map_err(|error| error.to_string())?;
        let plan = ProjectCommandPlan::new(ProjectCommandPlanOptions {
            project,
            command: ProjectCommand::Hook {
                name: "durable-command".to_owned(),
                arguments: self.arguments,
            },
            environment: self.environment,
            input: self.input,
            timeout: Duration::from_millis(self.timeout_milliseconds),
            browser_session: self.browser_session,
        })
        .map_err(|error| error.to_string())?;

        Ok(QueuedProjectCommand::new(
            operation_id,
            self.service_id,
            plan,
        ))
    }
}
