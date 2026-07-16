use super::{RawWorkflowMode, RawWorkflowStep};
use serde::{Deserialize, Serialize};

/// One explicitly invoked, ordered project workflow.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawWorkflowConfig {
    #[serde(default)]
    mode: RawWorkflowMode,
    steps: Vec<RawWorkflowStep>,
}

impl RawWorkflowConfig {
    pub(crate) const fn mode(&self) -> RawWorkflowMode {
        self.mode
    }

    pub(crate) fn steps(&self) -> &[RawWorkflowStep] {
        &self.steps
    }
}
