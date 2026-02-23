//! Logger initialization helpers.

use time::OffsetDateTime;
use tracing_subscriber::prelude::*;

use super::LoggerState;
use super::entry::LogEntry;
use super::layer::LaravelLayer;
use super::{LOGGER_STATE, LogLevel, Persistence, TRACING_ACTIVE};
use std::sync::atomic::Ordering;

pub(super) fn install_logger(quiet: bool) -> bool {
    drop(LOGGER_STATE.set(LoggerState::new(quiet)));
    let installed = install_tracing_subscriber();
    TRACING_ACTIVE.store(installed, Ordering::Relaxed);
    installed
}

pub(super) fn tracing_fallback_warning(timestamp: OffsetDateTime) -> LogEntry {
    LogEntry {
        timestamp,
        level: LogLevel::Warn,
        message: String::from(
            "[logger] tracing subscriber already set; using direct logger fallback",
        ),
        context: None,
        persistence: Persistence::Transient,
    }
}

fn install_tracing_subscriber() -> bool {
    let layer = LaravelLayer;
    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::set_global_default(subscriber).is_ok()
}
