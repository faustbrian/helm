use super::RawWorkflowStep;
use serde::Deserialize;

/// One explicitly invoked, ordered project workflow.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawWorkflowConfig {
    steps: Vec<RawWorkflowStep>,
}

impl RawWorkflowConfig {
    pub(crate) fn steps(&self) -> &[RawWorkflowStep] {
        &self.steps
    }
}
