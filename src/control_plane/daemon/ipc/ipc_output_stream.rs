use serde::{Deserialize, Serialize};

/// Origin of one binary-safe command output event.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum IpcOutputStream {
    Stdout,
    Stderr,
}
