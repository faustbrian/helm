use serde::{Deserialize, Serialize};

/// A safe user-facing tool invocation transported without shell parsing.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub(crate) enum IpcProjectCommand {
    Composer {
        arguments: Vec<String>,
    },
    Node {
        arguments: Vec<String>,
    },
    Bun {
        arguments: Vec<String>,
    },
    Hook {
        name: String,
        arguments: Vec<String>,
    },
}
