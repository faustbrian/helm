//! output tracing layer module.
//!
//! Contains tracing-layer event parsing and translation helpers.

use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

use super::entry::LogEntry;
use super::{LogLevel, Persistence, emit_entry_direct, now_local_timestamp};
mod fields;
use fields::{FieldCollector, parse_context_json};

pub(super) struct LaravelLayer;

impl<S> Layer<S> for LaravelLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = FieldCollector::default();
        event.record(&mut fields);

        let level = fields
            .level
            .as_deref()
            .and_then(LogLevel::from_laravel)
            .unwrap_or(LogLevel::from_tracing(*event.metadata().level()));

        let timestamp = now_local_timestamp();
        let message = fields.body.unwrap_or_default();
        let context = fields.context.and_then(parse_context_json);
        let persistence = fields
            .persistence
            .as_deref()
            .and_then(Persistence::from_str)
            .unwrap_or(Persistence::Transient);

        let entry = LogEntry {
            timestamp,
            level,
            message,
            context,
            persistence,
        };

        emit_entry_direct(&entry);
    }
}
