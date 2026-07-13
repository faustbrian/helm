use super::IpcEventKind;
use serde::{Deserialize, Serialize};

/// One ordered resumable event emitted by the singleton daemon.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcEvent {
    sequence: u64,
    operation_id: String,
    kind: IpcEventKind,
}

impl IpcEvent {
    pub(super) const fn new(sequence: u64, operation_id: String, kind: IpcEventKind) -> Self {
        Self {
            sequence,
            operation_id,
            kind,
        }
    }

    pub(crate) const fn sequence(&self) -> u64 {
        self.sequence
    }

    pub(crate) fn operation_id(&self) -> &str {
        &self.operation_id
    }

    pub(crate) const fn kind(&self) -> &IpcEventKind {
        &self.kind
    }
}
