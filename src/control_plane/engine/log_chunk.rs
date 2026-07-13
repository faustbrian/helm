/// Origin of one raw Engine log frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum LogStreamKind {
    Stdout,
    Stderr,
    Stdin,
    Console,
}

/// Raw container log bytes preserved without lossy text conversion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LogChunk {
    stream: LogStreamKind,
    bytes: Vec<u8>,
}

impl LogChunk {
    pub(crate) const fn new(stream: LogStreamKind, bytes: Vec<u8>) -> Self {
        Self { stream, bytes }
    }

    pub(crate) const fn stdout(bytes: Vec<u8>) -> Self {
        Self::new(LogStreamKind::Stdout, bytes)
    }

    pub(crate) const fn is_stderr(&self) -> bool {
        matches!(self.stream, LogStreamKind::Stderr)
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
