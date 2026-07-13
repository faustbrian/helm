use super::FilesystemEventWatcherError;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, TrySendError};

/// A bounded cross-platform signal for watched-root filesystem changes.
pub(crate) struct FilesystemEventWatcher {
    _watcher: RecommendedWatcher,
    changes: Receiver<()>,
}

impl FilesystemEventWatcher {
    /// Starts the platform-recommended recursive watcher for every root.
    pub(crate) fn new(roots: &[PathBuf]) -> Result<Self, FilesystemEventWatcherError> {
        let (changes, receiver) = std::sync::mpsc::sync_channel(1);
        let mut watcher = notify::recommended_watcher(move |event| signal_change(&changes, event))
            .map_err(FilesystemEventWatcherError::start)?;
        for root in roots {
            watcher
                .watch(root, RecursiveMode::Recursive)
                .map_err(|source| FilesystemEventWatcherError::watch(root, source))?;
        }

        Ok(Self {
            _watcher: watcher,
            changes: receiver,
        })
    }

    /// Coalesces all pending native notifications into one dirty signal.
    pub(crate) fn take_change(&self) -> Result<bool, FilesystemEventWatcherError> {
        let mut changed = false;
        loop {
            match self.changes.try_recv() {
                Ok(()) => changed = true,
                Err(TryRecvError::Empty) => return Ok(changed),
                Err(TryRecvError::Disconnected) => {
                    return Err(FilesystemEventWatcherError::disconnected());
                }
            }
        }
    }
}

fn signal_change(sender: &SyncSender<()>, event: notify::Result<Event>) {
    if !requires_registry_discovery(&event) {
        return;
    }
    match sender.try_send(()) {
        Ok(()) | Err(TrySendError::Full(())) | Err(TrySendError::Disconnected(())) => {}
    }
}

fn requires_registry_discovery(event: &notify::Result<Event>) -> bool {
    match event {
        Err(_) => true,
        Ok(event) => {
            event.need_rescan()
                || event.paths.iter().any(|path| {
                    path.file_name()
                        .is_some_and(|name| name == ".stackctl.yaml" || name == ".stackctl.toml")
                })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::signal_change;
    use notify::{Event, EventKind};
    use std::path::PathBuf;
    use std::sync::mpsc::TryRecvError;

    #[test]
    fn ordinary_project_file_changes_do_not_schedule_registry_discovery() {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let mut event = Event::new(EventKind::Any);
        event.paths.push(PathBuf::from("/projects/bill/README.md"));

        signal_change(&sender, Ok(event));

        assert_eq!(receiver.try_recv(), Err(TryRecvError::Empty));
    }
}
