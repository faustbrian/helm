//! output log entry module.
//!
//! Contains log entry formatting helpers.

use serde_json::Value;
use time::OffsetDateTime;

use super::colorize::colorize_message_tokens;
use super::{LogLevel, Persistence};

#[derive(Clone)]
pub(super) struct LogEntry {
    pub(super) timestamp: OffsetDateTime,
    pub(super) level: LogLevel,
    pub(super) message: String,
    pub(super) context: Option<Value>,
    pub(super) persistence: Persistence,
}

impl LogEntry {
    /// Renders file line for command execution.
    pub(super) fn render_file_line(&self) -> String {
        let timestamp = format_timestamp(self.timestamp);
        let level_label = self.level.padded_laravel_label();
        let line = format!("[{timestamp}] {level_label}: {}", self.message);
        append_context_json(line, self.context.as_ref())
    }

    /// Renders terminal line for command execution.
    pub(super) fn render_terminal_line(&self) -> String {
        let timestamp = format_timestamp(self.timestamp);
        let level_label = self.level.padded_laravel_label();
        let level_token = self.level.colorize_laravel_label(&level_label);
        let rendered_message = colorize_message_tokens(&self.message);
        let line = format!("[{timestamp}] {level_token}: {rendered_message}");
        append_context_json(line, self.context.as_ref())
    }
}

fn append_context_json(mut line: String, context: Option<&Value>) -> String {
    if let Some(json) = context {
        line.push_str("  ");
        line.push_str(&json.to_string());
    }
    line
}

fn format_timestamp(timestamp: OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        timestamp.year(),
        u8::from(timestamp.month()),
        timestamp.day(),
        timestamp.hour(),
        timestamp.minute(),
        timestamp.second()
    )
}
