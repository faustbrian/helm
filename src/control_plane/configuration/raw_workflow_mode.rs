use serde::{Deserialize, Serialize};

/// Declared authorization policy for one project workflow.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RawWorkflowMode {
    Automatic,
    #[default]
    Manual,
}
