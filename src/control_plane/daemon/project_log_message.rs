use super::ipc::IpcOutputStream;

/// One raw Engine log frame correlated with its project-visible service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectLogMessage {
    service: String,
    stream: IpcOutputStream,
    bytes: Vec<u8>,
}

impl ProjectLogMessage {
    pub(crate) const fn new(service: String, stream: IpcOutputStream, bytes: Vec<u8>) -> Self {
        Self {
            service,
            stream,
            bytes,
        }
    }

    pub(crate) fn service(&self) -> &str {
        &self.service
    }

    pub(crate) const fn stream(&self) -> IpcOutputStream {
        self.stream
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
