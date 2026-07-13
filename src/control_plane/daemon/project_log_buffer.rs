use std::collections::VecDeque;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

use super::ProjectLogBufferError;
use super::ipc::{IpcLogChunk, IpcLogSessionState, IpcOutputStream};

const LOG_CHUNK_BYTES: usize = 24 * 1024;

/// One bounded non-durable buffer backing a live project log session.
pub(crate) struct ProjectLogBuffer {
    capacity: usize,
    chunks: VecDeque<IpcLogChunk>,
    latest_sequence: u64,
    state: IpcLogSessionState,
}

impl ProjectLogBuffer {
    pub(crate) fn new(capacity: usize) -> Result<Self, ProjectLogBufferError> {
        if capacity == 0 {
            return Err(ProjectLogBufferError::InvalidCapacity);
        }

        Ok(Self {
            capacity,
            chunks: VecDeque::with_capacity(capacity),
            latest_sequence: 0,
            state: IpcLogSessionState::Starting,
        })
    }

    pub(crate) fn append(&mut self, service: &str, stream: IpcOutputStream, bytes: &[u8]) {
        self.start();
        for bytes in bytes.chunks(LOG_CHUNK_BYTES) {
            self.latest_sequence = self.latest_sequence.saturating_add(1);
            self.chunks.push_back(IpcLogChunk::new(
                self.latest_sequence,
                service.to_owned(),
                stream,
                STANDARD.encode(bytes),
            ));
            if self.chunks.len() > self.capacity {
                drop(self.chunks.pop_front());
            }
        }
    }

    pub(crate) fn poll(
        &self,
        after_sequence: Option<u64>,
        max_chunks: usize,
    ) -> Result<(Vec<IpcLogChunk>, u64, IpcLogSessionState), ProjectLogBufferError> {
        if max_chunks == 0 {
            return Err(ProjectLogBufferError::InvalidPageSize);
        }
        let requested = after_sequence.unwrap_or(0);
        if requested > self.latest_sequence {
            return Err(ProjectLogBufferError::CursorAhead {
                requested,
                latest: self.latest_sequence,
            });
        }
        if let Some(oldest) = self.chunks.front().map(IpcLogChunk::sequence)
            && requested.saturating_add(1) < oldest
        {
            return Err(ProjectLogBufferError::CursorExpired { requested, oldest });
        }

        let chunks = self
            .chunks
            .iter()
            .filter(|chunk| chunk.sequence() > requested)
            .take(max_chunks)
            .cloned()
            .collect::<Vec<_>>();
        let cursor = chunks
            .last()
            .map(IpcLogChunk::sequence)
            .unwrap_or(requested);

        let state = if self.state.terminal() && cursor < self.latest_sequence {
            IpcLogSessionState::Streaming
        } else {
            self.state.clone()
        };

        Ok((chunks, cursor, state))
    }

    pub(crate) fn start(&mut self) {
        if self.state == IpcLogSessionState::Starting {
            self.state = IpcLogSessionState::Streaming;
        }
    }

    pub(crate) fn complete(&mut self) {
        self.state = IpcLogSessionState::Completed;
    }

    pub(crate) fn fail(&mut self, code: impl Into<String>, message: impl Into<String>) {
        self.state = IpcLogSessionState::Failed {
            code: code.into(),
            message: message.into(),
        };
    }

    pub(crate) fn cancel(&mut self) {
        self.state = IpcLogSessionState::Cancelled;
    }
}
