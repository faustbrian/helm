use super::{IpcDiagnostic, IpcOutputStream};
use serde::{Deserialize, Serialize};

/// One stable lifecycle transition published for a daemon operation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub(crate) enum IpcEventKind {
    Accepted,
    Completed,
    Failed {
        code: String,
        message: String,
    },
    Output {
        stream: IpcOutputStream,
        data_base64: String,
    },
    Diagnostics {
        diagnostics: Vec<IpcDiagnostic>,
    },
    Cancelled,
}
