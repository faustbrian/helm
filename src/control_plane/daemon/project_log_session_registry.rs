use std::collections::{BTreeMap, VecDeque};

use super::ipc::{IpcLogChunk, IpcLogSessionState, IpcOutputStream};
use super::{ProjectLogBuffer, ProjectLogRequest, ProjectLogSessionRegistryError};

const DEFAULT_MAX_SESSIONS: usize = 32;
const DEFAULT_BUFFER_CHUNKS: usize = 1_024;

struct ProjectLogSession {
    request: ProjectLogRequest,
    buffer: ProjectLogBuffer,
}

/// Bounded non-durable registry for concurrent project log readers.
pub(crate) struct ProjectLogSessionRegistry {
    max_sessions: usize,
    buffer_chunks: usize,
    sessions: BTreeMap<String, ProjectLogSession>,
    pending: VecDeque<String>,
}

impl ProjectLogSessionRegistry {
    pub(crate) fn new(
        max_sessions: usize,
        buffer_chunks: usize,
    ) -> Result<Self, ProjectLogSessionRegistryError> {
        if max_sessions == 0 || buffer_chunks == 0 {
            return Err(ProjectLogSessionRegistryError::InvalidCapacity);
        }

        Ok(Self {
            max_sessions,
            buffer_chunks,
            sessions: BTreeMap::new(),
            pending: VecDeque::new(),
        })
    }

    pub(crate) fn open(
        &mut self,
        request: ProjectLogRequest,
    ) -> Result<(), ProjectLogSessionRegistryError> {
        let session_id = request.session_id().to_owned();
        if self.sessions.contains_key(&session_id) {
            return Err(ProjectLogSessionRegistryError::DuplicateSession { session_id });
        }
        if self.sessions.len() >= self.max_sessions {
            return Err(ProjectLogSessionRegistryError::CapacityReached {
                capacity: self.max_sessions,
            });
        }
        let buffer = ProjectLogBuffer::new(self.buffer_chunks).map_err(buffer_error)?;
        self.sessions
            .insert(session_id.clone(), ProjectLogSession { request, buffer });
        self.pending.push_back(session_id);

        Ok(())
    }

    pub(crate) fn take_pending(&mut self) -> Option<ProjectLogRequest> {
        while let Some(session_id) = self.pending.pop_front() {
            if let Some(session) = self.sessions.get(&session_id) {
                return Some(session.request.clone());
            }
        }

        None
    }

    pub(crate) fn start(&mut self, session_id: &str) -> Result<(), ProjectLogSessionRegistryError> {
        self.session_mut(session_id)?.buffer.start();
        Ok(())
    }

    pub(crate) fn append(
        &mut self,
        session_id: &str,
        service: &str,
        stream: IpcOutputStream,
        bytes: &[u8],
    ) -> Result<(), ProjectLogSessionRegistryError> {
        self.session_mut(session_id)?
            .buffer
            .append(service, stream, bytes);
        Ok(())
    }

    pub(crate) fn complete(
        &mut self,
        session_id: &str,
    ) -> Result<(), ProjectLogSessionRegistryError> {
        self.session_mut(session_id)?.buffer.complete();
        Ok(())
    }

    pub(crate) fn fail(
        &mut self,
        session_id: &str,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Result<(), ProjectLogSessionRegistryError> {
        self.session_mut(session_id)?.buffer.fail(code, message);
        Ok(())
    }

    pub(crate) fn cancel(
        &mut self,
        session_id: &str,
    ) -> Result<(), ProjectLogSessionRegistryError> {
        self.pending.retain(|pending| pending != session_id);
        self.session_mut(session_id)?.buffer.cancel();
        Ok(())
    }

    pub(crate) fn poll(
        &mut self,
        session_id: &str,
        after_sequence: Option<u64>,
        max_chunks: usize,
    ) -> Result<(Vec<IpcLogChunk>, u64, IpcLogSessionState), ProjectLogSessionRegistryError> {
        let result = self
            .sessions
            .get(session_id)
            .ok_or_else(|| ProjectLogSessionRegistryError::UnknownSession {
                session_id: session_id.to_owned(),
            })?
            .buffer
            .poll(after_sequence, max_chunks)
            .map_err(buffer_error)?;
        if result.2.terminal() {
            drop(self.sessions.remove(session_id));
        }

        Ok(result)
    }

    fn session_mut(
        &mut self,
        session_id: &str,
    ) -> Result<&mut ProjectLogSession, ProjectLogSessionRegistryError> {
        self.sessions.get_mut(session_id).ok_or_else(|| {
            ProjectLogSessionRegistryError::UnknownSession {
                session_id: session_id.to_owned(),
            }
        })
    }
}

impl Default for ProjectLogSessionRegistry {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_SESSIONS, DEFAULT_BUFFER_CHUNKS)
            .expect("default project log session capacities are valid")
    }
}

fn buffer_error(error: impl std::fmt::Display) -> ProjectLogSessionRegistryError {
    ProjectLogSessionRegistryError::Buffer {
        detail: error.to_string(),
    }
}
