use serde::{Deserialize, Serialize};

/// Whitelisted PHP project tool transported over singleton IPC.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum IpcPhpTool {
    PhpStan,
    Ecs,
    PhpCsFixer,
    Psalm,
    Pint,
    Pest,
    PhpUnit,
    Rector,
}
