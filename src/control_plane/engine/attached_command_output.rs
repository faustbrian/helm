use std::fmt::{Debug, Formatter};

/// Bounded raw output from one completed attached Engine command.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct AttachedCommandOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl Debug for AttachedCommandOutput {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AttachedCommandOutput")
            .field("stdout_bytes", &self.stdout.len())
            .field("stderr_bytes", &self.stderr.len())
            .finish()
    }
}

impl AttachedCommandOutput {
    pub(super) const fn new() -> Self {
        Self {
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    pub(crate) fn stdout(&self) -> &[u8] {
        &self.stdout
    }

    pub(crate) fn stderr(&self) -> &[u8] {
        &self.stderr
    }

    pub(super) fn extend_stdout(&mut self, bytes: &[u8]) {
        self.stdout.extend_from_slice(bytes);
    }

    pub(super) fn extend_stderr(&mut self, bytes: &[u8]) {
        self.stderr.extend_from_slice(bytes);
    }

    pub(super) fn into_stdout(self) -> Vec<u8> {
        self.stdout
    }

    pub(super) fn len(&self) -> usize {
        self.stdout.len().saturating_add(self.stderr.len())
    }
}
