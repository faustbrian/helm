use serde::Serialize;

/// One exact JavaScript runtime embedded in a reusable Linux runtime image.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "runtime", rename_all = "lowercase")]
pub(crate) enum JavaScriptRuntimeSpec {
    Node { version: String },
    Bun { version: String },
}

impl JavaScriptRuntimeSpec {
    pub(super) fn version(&self) -> &str {
        match self {
            Self::Node { version } | Self::Bun { version } => version,
        }
    }
}
