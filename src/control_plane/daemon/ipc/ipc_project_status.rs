use serde::{Deserialize, Serialize};

use super::IpcResourceStatus;

/// Secret-free durable status for one exact registered v8 project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcProjectStatus {
    project: String,
    routes: Vec<String>,
    resources: Vec<IpcResourceStatus>,
}

impl IpcProjectStatus {
    pub(crate) const fn new(
        project: String,
        routes: Vec<String>,
        resources: Vec<IpcResourceStatus>,
    ) -> Self {
        Self {
            project,
            routes,
            resources,
        }
    }

    pub(crate) fn project(&self) -> &str {
        &self.project
    }

    pub(crate) fn routes(&self) -> &[String] {
        &self.routes
    }

    pub(crate) fn resources(&self) -> &[IpcResourceStatus] {
        &self.resources
    }
}
