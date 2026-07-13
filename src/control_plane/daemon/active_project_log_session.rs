use super::ProjectLogMessage;
use crate::control_plane::engine::EngineError;

/// One running Engine log producer and its bounded daemon-side channel.
pub(crate) struct ActiveProjectLogSession {
    receiver: tokio::sync::mpsc::Receiver<ProjectLogMessage>,
    task: tokio::task::JoinHandle<Result<(), EngineError>>,
}

impl ActiveProjectLogSession {
    pub(crate) const fn new(
        receiver: tokio::sync::mpsc::Receiver<ProjectLogMessage>,
        task: tokio::task::JoinHandle<Result<(), EngineError>>,
    ) -> Self {
        Self { receiver, task }
    }

    pub(crate) fn drain(&mut self) -> Vec<ProjectLogMessage> {
        let mut messages = Vec::new();
        while let Ok(message) = self.receiver.try_recv() {
            messages.push(message);
        }
        messages
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.task.is_finished()
    }

    pub(crate) fn abort(&self) {
        self.task.abort();
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        tokio::sync::mpsc::Receiver<ProjectLogMessage>,
        tokio::task::JoinHandle<Result<(), EngineError>>,
    ) {
        (self.receiver, self.task)
    }
}
