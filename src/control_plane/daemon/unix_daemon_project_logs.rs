use std::time::Instant;

use super::{
    ActiveProjectLogSession, EngineConnectionOutcome, UnixDaemonRuntime, execute_project_logs,
};

const LOG_CHANNEL_CAPACITY: usize = 64;

impl UnixDaemonRuntime {
    pub(super) fn stop_project_logs_for_shutdown(&mut self) {
        for active in self.active_project_logs.values() {
            active.abort();
        }
        self.active_project_logs.clear();
        self.engine_runtime.block_on(tokio::task::yield_now());
    }

    /// Advances concurrent read-only Engine log sessions without durable output.
    pub(super) fn drive_project_logs(&mut self, now: Instant) {
        for session_id in self.project_logs.expire_idle(now) {
            tracing::debug!(session_id, "expired idle project log session");
        }
        self.cancel_project_log_sessions();
        self.engine_runtime.block_on(tokio::task::yield_now());
        self.drain_project_log_messages();
        self.finish_project_log_sessions();
        self.cancel_project_log_sessions();

        match self
            .engine_runtime
            .block_on(self.engine_connection.poll(now))
        {
            EngineConnectionOutcome::Unavailable { retry, detail } => {
                tracing::debug!(
                    attempt = retry.attempt(),
                    retry_milliseconds = retry.duration().as_millis(),
                    error = %detail,
                    "project logs are waiting for the selected Engine"
                );
                return;
            }
            EngineConnectionOutcome::Connected | EngineConnectionOutcome::BackingOff { .. } => {}
        }
        let Some(engine) = self.engine_connection.engine().cloned() else {
            return;
        };
        let installation_id = self
            .global_network_request
            .metadata()
            .installation_id()
            .to_owned();
        let schema_version = self.global_network_request.metadata().schema_version();

        while let Some(request) = self.project_logs.take_pending() {
            let session_id = request.session_id().to_owned();
            if let Err(error) = self.project_logs.start(&session_id) {
                tracing::error!(session_id, error = %error, "project log session could not start");
                continue;
            }
            let (sender, receiver) = tokio::sync::mpsc::channel(LOG_CHANNEL_CAPACITY);
            let task = self.engine_runtime.spawn(execute_project_logs(
                engine.clone(),
                request,
                installation_id.clone(),
                schema_version,
                sender,
            ));
            self.active_project_logs
                .insert(session_id, ActiveProjectLogSession::new(receiver, task));
        }
        self.engine_runtime.block_on(tokio::task::yield_now());
    }

    fn drain_project_log_messages(&mut self) {
        for (session_id, active) in &mut self.active_project_logs {
            for message in active.drain() {
                if let Err(error) = self.project_logs.append(
                    session_id,
                    message.service(),
                    message.stream(),
                    message.bytes(),
                ) {
                    tracing::error!(session_id, error = %error, "project log chunk was dropped");
                }
            }
        }
    }

    fn finish_project_log_sessions(&mut self) {
        let finished = self
            .active_project_logs
            .iter()
            .filter(|(_, active)| active.is_finished())
            .map(|(session_id, _)| session_id.clone())
            .collect::<Vec<_>>();
        for session_id in finished {
            let Some(active) = self.active_project_logs.remove(&session_id) else {
                continue;
            };
            let (mut receiver, task) = active.into_parts();
            let outcome = self.engine_runtime.block_on(task);
            while let Ok(message) = receiver.try_recv() {
                drop(self.project_logs.append(
                    &session_id,
                    message.service(),
                    message.stream(),
                    message.bytes(),
                ));
            }
            let result = match outcome {
                Ok(Ok(())) => self.project_logs.complete(&session_id),
                Ok(Err(error)) => {
                    self.project_logs
                        .fail(&session_id, "project_logs_failed", error.to_string())
                }
                Err(error) => self.project_logs.fail(
                    &session_id,
                    "project_log_task_failed",
                    error.to_string(),
                ),
            };
            if let Err(error) = result {
                tracing::error!(session_id, error = %error, "project log session could not finish");
            }
        }
    }

    fn cancel_project_log_sessions(&mut self) {
        let cancelled = self
            .active_project_logs
            .keys()
            .filter(|session_id| self.project_logs.should_stop(session_id))
            .cloned()
            .collect::<Vec<_>>();
        for session_id in cancelled {
            if let Some(active) = self.active_project_logs.remove(&session_id) {
                active.abort();
            }
        }
    }
}
