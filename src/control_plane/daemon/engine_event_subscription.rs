use super::{EngineEventObservation, RetryBackoff, RetryBackoffOptions};
use crate::control_plane::engine::{ContainerEvent, ContainerEventCursor, ContainerEventSource};
use futures_util::StreamExt;
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

const EVENT_CHANNEL_CAPACITY: usize = 256;
const MAX_EVENTS_PER_POLL: usize = 256;

enum EngineEventMessage {
    Event(ContainerEvent),
    Failed(String),
}

/// Persistent Engine event subscription with cursor-based bounded reconnects.
pub(crate) struct EngineEventSubscription {
    installation_id: String,
    cursor: ContainerEventCursor,
    receiver: Option<Receiver<EngineEventMessage>>,
    task: Option<JoinHandle<()>>,
    retry: RetryBackoff,
    retry_at: Option<Instant>,
}

impl EngineEventSubscription {
    pub(crate) fn new(installation_id: &str) -> Result<Self, super::RetryBackoffError> {
        Ok(Self {
            installation_id: installation_id.to_owned(),
            cursor: ContainerEventCursor::beginning(),
            receiver: None,
            task: None,
            retry: RetryBackoff::new(
                format!("engine-events:{installation_id}"),
                RetryBackoffOptions::new(Duration::from_millis(500), Duration::from_secs(30))?,
            )?,
            retry_at: None,
        })
    }

    pub(crate) fn poll<E>(
        &mut self,
        runtime: &Runtime,
        source: Option<E>,
        now: Instant,
    ) -> EngineEventObservation
    where
        E: ContainerEventSource + Clone + Send + Sync + 'static,
    {
        if self.task.is_none()
            && self.retry_at.is_none_or(|retry_at| now >= retry_at)
            && let Some(source) = source
        {
            self.start(runtime, source);
        }
        runtime.block_on(async { tokio::task::yield_now().await });

        let mut count = 0;
        let mut failure = None;
        if let Some(receiver) = &self.receiver {
            while count < MAX_EVENTS_PER_POLL {
                match receiver.try_recv() {
                    Ok(EngineEventMessage::Event(event)) => {
                        self.cursor = self.cursor.clone().advance(&event);
                        count += 1;
                    }
                    Ok(EngineEventMessage::Failed(detail)) => {
                        failure = Some(detail);
                        break;
                    }
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                }
            }
        }
        if count > 0 {
            self.retry.reset();
        }
        let disconnected =
            failure.is_some() || self.task.as_ref().is_some_and(JoinHandle::is_finished);
        if disconnected {
            self.receiver = None;
            self.task = None;
            let delay = self.retry.next_delay();
            self.retry_at = Some(now + delay.duration());
        }
        if count > 0 {
            return EngineEventObservation::Events { count };
        }
        if disconnected {
            return EngineEventObservation::Disconnected {
                detail: failure.unwrap_or_else(|| "managed Engine event stream closed".to_owned()),
            };
        }

        EngineEventObservation::Idle
    }

    fn start<E>(&mut self, runtime: &Runtime, source: E)
    where
        E: ContainerEventSource + Clone + Send + Sync + 'static,
    {
        let (sender, receiver) = sync_channel(EVENT_CHANNEL_CAPACITY);
        let installation_id = self.installation_id.clone();
        let cursor = self.cursor.clone();
        self.receiver = Some(receiver);
        self.retry_at = None;
        self.task = Some(runtime.spawn(stream_events(source, installation_id, cursor, sender)));
    }
}

async fn stream_events<E>(
    source: E,
    installation_id: String,
    cursor: ContainerEventCursor,
    sender: SyncSender<EngineEventMessage>,
) where
    E: ContainerEventSource + Send + Sync,
{
    let mut stream = source.stream_managed(&installation_id, cursor);
    while let Some(result) = stream.next().await {
        let message = match result {
            Ok(event) => EngineEventMessage::Event(event),
            Err(error) => {
                drop(sender.try_send(EngineEventMessage::Failed(error.to_string())));
                return;
            }
        };
        drop(sender.try_send(message));
    }
}
