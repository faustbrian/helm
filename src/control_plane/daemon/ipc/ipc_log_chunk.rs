use serde::{Deserialize, Serialize};

use super::IpcOutputStream;

/// One ordered binary-safe chunk from a project log session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcLogChunk {
    sequence: u64,
    service: String,
    stream: IpcOutputStream,
    data_base64: String,
}

impl IpcLogChunk {
    pub(crate) const fn new(
        sequence: u64,
        service: String,
        stream: IpcOutputStream,
        data_base64: String,
    ) -> Self {
        Self {
            sequence,
            service,
            stream,
            data_base64,
        }
    }

    pub(crate) const fn sequence(&self) -> u64 {
        self.sequence
    }

    pub(crate) fn service(&self) -> &str {
        &self.service
    }

    pub(crate) const fn stream(&self) -> IpcOutputStream {
        self.stream
    }

    pub(crate) fn data_base64(&self) -> &str {
        &self.data_base64
    }
}
