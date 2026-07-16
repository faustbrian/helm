use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use super::ipc::{IpcLogChunk, IpcLogSessionState, IpcOutputStream};
use super::{ProjectLogBuffer, ProjectLogRequest, ProjectLogSessionRegistryError};

const DEFAULT_MAX_SESSIONS: usize = 32;
const DEFAULT_BUFFER_CHUNKS: usize = 1_024;
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

struct ProjectLogSession {
    request: ProjectLogRequest,
    buffer: ProjectLogBuffer,
    last_activity: Instant,
}

/// Bounded non-durable registry for concurrent project log readers.
pub(crate) struct ProjectLogSessionRegistry {
    max_sessions: usize,
    buffer_chunks: usize,
    idle_timeout: Duration,
    sessions: BTreeMap<String, ProjectLogSession>,
    pending: VecDeque<String>,
}

impl ProjectLogSessionRegistry {
    #[cfg(test)]
    pub(crate) fn new(
        max_sessions: usize,
        buffer_chunks: usize,
    ) -> Result<Self, ProjectLogSessionRegistryError> {
        Self::with_idle_timeout(max_sessions, buffer_chunks, DEFAULT_IDLE_TIMEOUT)
    }

    #[cfg(test)]
    pub(crate) fn with_idle_timeout(
        max_sessions: usize,
        buffer_chunks: usize,
        idle_timeout: Duration,
    ) -> Result<Self, ProjectLogSessionRegistryError> {
        if max_sessions == 0 || buffer_chunks == 0 {
            return Err(ProjectLogSessionRegistryError::InvalidCapacity);
        }
        if idle_timeout.is_zero() {
            return Err(ProjectLogSessionRegistryError::InvalidIdleTimeout);
        }

        Ok(Self {
            max_sessions,
            buffer_chunks,
            idle_timeout,
            sessions: BTreeMap::new(),
            pending: VecDeque::new(),
        })
    }

    pub(crate) fn open(
        &mut self,
        request: ProjectLogRequest,
    ) -> Result<(), ProjectLogSessionRegistryError> {
        self.open_at(request, Instant::now())
    }

    pub(crate) fn open_at(
        &mut self,
        request: ProjectLogRequest,
        now: Instant,
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
        self.sessions.insert(
            session_id.clone(),
            ProjectLogSession {
                request,
                buffer,
                last_activity: now,
            },
        );
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
        self.sessions.remove(session_id).ok_or_else(|| {
            ProjectLogSessionRegistryError::UnknownSession {
                session_id: session_id.to_owned(),
            }
        })?;
        Ok(())
    }

    pub(crate) fn should_stop(&self, session_id: &str) -> bool {
        !self.sessions.contains_key(session_id)
    }

    pub(crate) fn expire_idle(&mut self, now: Instant) -> Vec<String> {
        let expired = self
            .sessions
            .iter()
            .filter(|(_, session)| {
                now.saturating_duration_since(session.last_activity) >= self.idle_timeout
            })
            .map(|(session_id, _)| session_id.clone())
            .collect::<Vec<_>>();
        for session_id in &expired {
            self.pending.retain(|pending| pending != session_id);
            drop(self.sessions.remove(session_id));
        }

        expired
    }

    pub(crate) fn poll(
        &mut self,
        session_id: &str,
        after_sequence: Option<u64>,
        max_chunks: usize,
    ) -> Result<(Vec<IpcLogChunk>, u64, IpcLogSessionState), ProjectLogSessionRegistryError> {
        self.poll_at(session_id, after_sequence, max_chunks, Instant::now())
    }

    pub(crate) fn poll_at(
        &mut self,
        session_id: &str,
        after_sequence: Option<u64>,
        max_chunks: usize,
        now: Instant,
    ) -> Result<(Vec<IpcLogChunk>, u64, IpcLogSessionState), ProjectLogSessionRegistryError> {
        let session = self.sessions.get_mut(session_id).ok_or_else(|| {
            ProjectLogSessionRegistryError::UnknownSession {
                session_id: session_id.to_owned(),
            }
        })?;
        let result = session
            .buffer
            .poll(after_sequence, max_chunks)
            .map_err(buffer_error)?;
        session.last_activity = now;
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
        Self {
            max_sessions: DEFAULT_MAX_SESSIONS,
            buffer_chunks: DEFAULT_BUFFER_CHUNKS,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            sessions: BTreeMap::new(),
            pending: VecDeque::new(),
        }
    }
}

fn buffer_error(error: impl std::fmt::Display) -> ProjectLogSessionRegistryError {
    ProjectLogSessionRegistryError::Buffer {
        detail: error.to_string(),
    }
}
