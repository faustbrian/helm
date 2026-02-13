//! output module.
//!
//! Contains output logic used by Helm command workflows.

use std::fs::{File, OpenOptions, create_dir_all};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use colored::Colorize;
use serde_json::Value;
use time::OffsetDateTime;
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;

static LOGGER_STATE: OnceLock<LoggerState> = OnceLock::new();
static TRACING_ACTIVE: AtomicBool = AtomicBool::new(false);
const LEVEL_COLUMN_WIDTH: usize = 15;
const TOKEN_COLUMN_WIDTHS: [usize; 2] = [16, 16];

#[derive(Clone, Copy)]
pub(crate) enum LogLevel {
    Emergency,
    Alert,
    Critical,
    Error,
    Warn,
    Notice,
    Info,
    Debug,
    Success,
}

#[derive(Clone, Copy)]
pub(crate) enum Channel {
    Out,
    Err,
}

#[derive(Clone, Copy)]
pub(crate) enum Persistence {
    Transient,
    Persistent,
}

impl Persistence {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Transient => "transient",
            Self::Persistent => "persistent",
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct LoggerOptions {
    quiet: bool,
}

impl LoggerOptions {
    pub(crate) const fn new(quiet: bool) -> Self {
        Self { quiet }
    }
}

struct LoggerState {
    quiet: bool,
    file_state: Mutex<FileState>,
}

struct FileState {
    directory: Option<PathBuf>,
    day: Option<String>,
    file: Option<File>,
}

impl LoggerState {
    fn new(options: LoggerOptions) -> Self {
        Self {
            quiet: options.quiet,
            file_state: Mutex::new(FileState {
                directory: log_dir_path(),
                day: None,
                file: None,
            }),
        }
    }

    fn emit_terminal(&self, entry: &LogEntry) {
        if self.quiet && !entry.level.visible_when_quiet() {
            return;
        }
        println!("{}", entry.render_terminal_line());
    }

    fn persist(&self, entry: &LogEntry) {
        if !matches!(entry.persistence, Persistence::Persistent) {
            return;
        }

        let Ok(mut state) = self.file_state.lock() else {
            return;
        };
        let Some(directory) = state.directory.clone() else {
            return;
        };

        let day = entry.timestamp.date().to_string();
        if state.day.as_ref() != Some(&day) || state.file.is_none() {
            if create_dir_all(&directory).is_err() {
                return;
            }
            let file_path = directory.join(format!("{day}.log"));
            let Ok(file) = OpenOptions::new().create(true).append(true).open(file_path) else {
                return;
            };
            state.file = Some(file);
            state.day = Some(day);
        }

        if let Some(file) = state.file.as_mut() {
            drop(writeln!(file, "{}", entry.render_file_line()));
        }
    }
}

struct LaravelLayer;

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

#[derive(Clone)]
struct LogEntry {
    timestamp: OffsetDateTime,
    level: LogLevel,
    message: String,
    context: Option<Value>,
    persistence: Persistence,
}

impl LogEntry {
    /// Renders file line for command execution.
    fn render_file_line(&self) -> String {
        let timestamp = format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.timestamp.year(),
            u8::from(self.timestamp.month()),
            self.timestamp.day(),
            self.timestamp.hour(),
            self.timestamp.minute(),
            self.timestamp.second()
        );

        let level_label = self.level.padded_laravel_label();
        let mut line = format!("[{timestamp}] {level_label}: {}", self.message);

        if let Some(json) = self.context.as_ref() {
            line.push_str("  ");
            line.push_str(&json.to_string());
        }

        line
    }

    /// Renders terminal line for command execution.
    fn render_terminal_line(&self) -> String {
        let timestamp = format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.timestamp.year(),
            u8::from(self.timestamp.month()),
            self.timestamp.day(),
            self.timestamp.hour(),
            self.timestamp.minute(),
            self.timestamp.second()
        );

        let level_label = self.level.padded_laravel_label();
        let level_token = self.level.colorize_laravel_label(&level_label);
        let rendered_message = colorize_message_tokens(&self.message);
        let mut line = format!("[{timestamp}] {level_token}: {rendered_message}");
        if let Some(json) = self.context.as_ref() {
            line.push_str("  ");
            line.push_str(&json.to_string());
        }
        line
    }
}

impl LogLevel {
    const fn as_laravel(self) -> &'static str {
        match self {
            Self::Emergency => "EMERGENCY",
            Self::Alert => "ALERT",
            Self::Critical => "CRITICAL",
            Self::Error => "ERROR",
            Self::Warn => "WARNING",
            Self::Notice => "NOTICE",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
            Self::Success => "SUCCESS",
        }
    }

    const fn as_tracing_level(self) -> Level {
        match self {
            Self::Emergency | Self::Alert | Self::Critical | Self::Error => Level::ERROR,
            Self::Warn => Level::WARN,
            Self::Notice | Self::Info | Self::Success => Level::INFO,
            Self::Debug => Level::DEBUG,
        }
    }

    const fn from_tracing(level: Level) -> Self {
        match level {
            Level::ERROR => Self::Error,
            Level::WARN => Self::Warn,
            Level::INFO => Self::Info,
            Level::DEBUG | Level::TRACE => Self::Debug,
        }
    }

    fn from_laravel(value: &str) -> Option<Self> {
        match value {
            "EMERGENCY" => Some(Self::Emergency),
            "ALERT" => Some(Self::Alert),
            "CRITICAL" => Some(Self::Critical),
            "ERROR" => Some(Self::Error),
            "WARNING" => Some(Self::Warn),
            "NOTICE" => Some(Self::Notice),
            "INFO" => Some(Self::Info),
            "DEBUG" => Some(Self::Debug),
            "SUCCESS" => Some(Self::Success),
            _ => None,
        }
    }

    const fn visible_when_quiet(self) -> bool {
        matches!(
            self,
            Self::Emergency | Self::Alert | Self::Critical | Self::Error | Self::Warn
        )
    }

    fn padded_laravel_label(self) -> String {
        let level_width = LEVEL_COLUMN_WIDTH.saturating_sub(6);
        let level = self.as_laravel();
        let dots = ".".repeat(level_width.saturating_sub(level.len()));
        format!("local.{level}{dots}")
    }

    fn colorize_laravel_label(self, token: &str) -> String {
        match self {
            Self::Emergency | Self::Alert | Self::Critical | Self::Error => token.red().to_string(),
            Self::Warn => token.yellow().to_string(),
            Self::Notice | Self::Info => token.blue().to_string(),
            Self::Debug => token.dimmed().to_string(),
            Self::Success => token.green().to_string(),
        }
    }
}

impl Persistence {
    fn from_str(value: &str) -> Option<Self> {
        match value {
            "transient" => Some(Self::Transient),
            "persistent" => Some(Self::Persistent),
            _ => None,
        }
    }
}

#[derive(Default)]
struct FieldCollector {
    body: Option<String>,
    level: Option<String>,
    context: Option<String>,
    persistence: Option<String>,
}

impl tracing::field::Visit for FieldCollector {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.record(field.name(), value.to_owned());
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.record(field.name(), format!("{value:?}"));
    }
}

impl FieldCollector {
    fn record(&mut self, key: &str, value: String) {
        match key {
            "body" => self.body = Some(value),
            "log_level" => self.level = Some(value),
            "context" => self.context = Some(value),
            "persistence" => self.persistence = Some(value),
            _ => {}
        }
    }
}

pub(crate) fn init(options: LoggerOptions) {
    drop(LOGGER_STATE.set(LoggerState::new(options)));
    let layer = LaravelLayer;
    let subscriber = tracing_subscriber::registry().with(layer);
    let installed = tracing::subscriber::set_global_default(subscriber).is_ok();
    TRACING_ACTIVE.store(installed, Ordering::Relaxed);

    if !installed {
        let warning = LogEntry {
            timestamp: now_local_timestamp(),
            level: LogLevel::Warn,
            message: String::from(
                "[logger] tracing subscriber already set; using direct logger fallback",
            ),
            context: None,
            persistence: Persistence::Transient,
        };
        emit_entry_direct(&warning);
    }
}

pub(crate) fn event(scope: &str, level: LogLevel, message: &str, persistence: Persistence) {
    event_with_context(scope, level, message, None, persistence);
}

pub(crate) fn event_with_context(
    scope: &str,
    level: LogLevel,
    message: &str,
    context: Option<Value>,
    persistence: Persistence,
) {
    let normalized = normalize_log_message(level, message);
    emit_entry(
        level,
        prefixed_scope_message(scope, &normalized),
        context,
        persistence,
    );
}

pub(crate) fn stream(scope: &str, channel: Channel, message: &str, persistence: Persistence) {
    stream_with_context(scope, channel, message, None, persistence);
}

pub(crate) fn stream_with_context(
    scope: &str,
    _channel: Channel,
    message: &str,
    context: Option<Value>,
    persistence: Persistence,
) {
    let normalized = strip_leading_bracket_timestamp(message);
    let sanitized = strip_ansi_codes(normalized);
    let (embedded_level, stripped_message) = strip_leading_laravel_prefix(&sanitized);
    let level = embedded_level.unwrap_or(LogLevel::Info);
    let standardized = normalize_log_message(level, stripped_message);
    let message = format!("[{scope}] {standardized}");
    emit_entry(level, message, context, persistence);
}

fn emit_entry(level: LogLevel, message: String, context: Option<Value>, persistence: Persistence) {
    if !TRACING_ACTIVE.load(Ordering::Relaxed) {
        let entry = LogEntry {
            timestamp: now_local_timestamp(),
            level,
            message,
            context,
            persistence,
        };
        emit_entry_direct(&entry);
        return;
    }

    let tracing_level = level.as_tracing_level();
    let context_string = context.map(|value| value.to_string());

    match (tracing_level, context_string) {
        (Level::ERROR, Some(json)) => tracing::event!(
            Level::ERROR,
            log_level = level.as_laravel(),
            body = %message,
            context = %json,
            persistence = persistence.as_str(),
            "helm_log"
        ),
        (Level::WARN, Some(json)) => tracing::event!(
            Level::WARN,
            log_level = level.as_laravel(),
            body = %message,
            context = %json,
            persistence = persistence.as_str(),
            "helm_log"
        ),
        (Level::INFO, Some(json)) => tracing::event!(
            Level::INFO,
            log_level = level.as_laravel(),
            body = %message,
            context = %json,
            persistence = persistence.as_str(),
            "helm_log"
        ),
        (Level::DEBUG, Some(json)) => tracing::event!(
            Level::DEBUG,
            log_level = level.as_laravel(),
            body = %message,
            context = %json,
            persistence = persistence.as_str(),
            "helm_log"
        ),
        (Level::TRACE, Some(json)) => tracing::event!(
            Level::TRACE,
            log_level = level.as_laravel(),
            body = %message,
            context = %json,
            persistence = persistence.as_str(),
            "helm_log"
        ),
        (Level::ERROR, None) => tracing::event!(
            Level::ERROR,
            log_level = level.as_laravel(),
            body = %message,
            persistence = persistence.as_str(),
            "helm_log"
        ),
        (Level::WARN, None) => tracing::event!(
            Level::WARN,
            log_level = level.as_laravel(),
            body = %message,
            persistence = persistence.as_str(),
            "helm_log"
        ),
        (Level::INFO, None) => tracing::event!(
            Level::INFO,
            log_level = level.as_laravel(),
            body = %message,
            persistence = persistence.as_str(),
            "helm_log"
        ),
        (Level::DEBUG, None) => tracing::event!(
            Level::DEBUG,
            log_level = level.as_laravel(),
            body = %message,
            persistence = persistence.as_str(),
            "helm_log"
        ),
        (Level::TRACE, None) => tracing::event!(
            Level::TRACE,
            log_level = level.as_laravel(),
            body = %message,
            persistence = persistence.as_str(),
            "helm_log"
        ),
    };
}

fn emit_entry_direct(entry: &LogEntry) {
    if let Some(logger) = LOGGER_STATE.get() {
        logger.emit_terminal(entry);
        logger.persist(entry);
        return;
    }

    println!("{}", entry.render_terminal_line());
}

fn prefixed_scope_message(scope: &str, message: &str) -> String {
    format!("[{scope}] {message}")
}

/// Parses context json into strongly typed values.
fn parse_context_json(value: String) -> Option<Value> {
    match serde_json::from_str::<Value>(&value) {
        Ok(parsed) => Some(parsed),
        Err(_) => None,
    }
}

fn now_local_timestamp() -> OffsetDateTime {
    match OffsetDateTime::now_local() {
        Ok(local) => local,
        Err(_) => OffsetDateTime::now_utc(),
    }
}

fn log_dir_path() -> Option<PathBuf> {
    let Ok(home) = std::env::var("HOME") else {
        return None;
    };
    Some(PathBuf::from(home).join(".config/helm/logs"))
}

fn strip_leading_bracket_timestamp(message: &str) -> &str {
    if let Some(rest) = message.strip_prefix('[')
        && let Some((inside, tail)) = rest.split_once(']')
        && is_fractional_unix_timestamp(inside)
    {
        return tail.trim_start();
    }

    message
}

fn strip_leading_laravel_prefix(message: &str) -> (Option<LogLevel>, &str) {
    let Some(rest) = message.strip_prefix('[') else {
        return (None, message);
    };
    let Some((timestamp, suffix)) = rest.split_once("] ") else {
        return (None, message);
    };
    if !is_laravel_timestamp(timestamp) {
        return (None, message);
    }

    let Some(after_local) = suffix.strip_prefix("local.") else {
        return (None, message);
    };
    let Some((level_padded, body)) = after_local.split_once(": ") else {
        return (None, message);
    };
    let level_raw = level_padded.trim_end_matches('.');

    (LogLevel::from_laravel(level_raw), body)
}

/// Returns whether a log line begins with a Laravel-style timestamp.
fn is_laravel_timestamp(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 19 {
        return false;
    }

    let separators = [(4, b'-'), (7, b'-'), (10, b' '), (13, b':'), (16, b':')];
    for (index, token) in separators {
        if bytes[index] != token {
            return false;
        }
    }

    let digit_indices = [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18];
    digit_indices
        .iter()
        .all(|&index| bytes[index].is_ascii_digit())
}

/// Returns whether a token is a fractional Unix timestamp.
fn is_fractional_unix_timestamp(value: &str) -> bool {
    let mut parts = value.split('.');
    let Some(seconds) = parts.next() else {
        return false;
    };
    let Some(millis) = parts.next() else {
        return false;
    };
    if parts.next().is_some() {
        return false;
    }

    !seconds.is_empty()
        && !millis.is_empty()
        && seconds.chars().all(|c| c.is_ascii_digit())
        && millis.chars().all(|c| c.is_ascii_digit())
}

/// Normalizes log message into a canonical form.
fn normalize_log_message(level: LogLevel, message: &str) -> String {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return String::from("(empty)");
    }

    let mut normalized = String::with_capacity(trimmed.len());
    let mut in_space = false;
    for ch in trimmed.chars() {
        if ch.is_whitespace() {
            if !in_space {
                normalized.push(' ');
                in_space = true;
            }
            continue;
        }
        normalized.push(ch);
        in_space = false;
    }

    while normalized.ends_with('.') {
        normalized.pop();
    }

    let rewritten = rewrite_message_for_level(level, normalized);
    uppercase_first_message_letter(rewritten)
}

fn rewrite_message_for_level(level: LogLevel, message: String) -> String {
    let mut value = strip_simple_single_quotes(&message);

    if let Some(rest) = value.strip_prefix("Skipping ") {
        value = format!("Skipped {rest}");
    }

    if matches!(
        level,
        LogLevel::Error | LogLevel::Critical | LogLevel::Alert | LogLevel::Emergency
    ) {
        if let Some(rest) = value.strip_prefix("Error: ") {
            value = format!("Operation failed: {rest}");
        } else if let Some(rest) = value.strip_prefix("Failed to ") {
            if let Some((action, reason)) = rest.split_once(": ") {
                value = format!("{action} failed: {reason}");
            } else {
                value = format!("{rest} failed");
            }
        }
    }

    value
}

fn strip_simple_single_quotes(message: &str) -> String {
    let mut output = String::with_capacity(message.len());
    let bytes = message.as_bytes();
    let mut index = 0_usize;

    while index < bytes.len() {
        if bytes[index] != b'\'' {
            output.push(char::from(bytes[index]));
            index += 1;
            continue;
        }

        let start = index + 1;
        let mut end = start;
        while end < bytes.len() && bytes[end] != b'\'' {
            end += 1;
        }
        if end >= bytes.len() {
            output.push('\'');
            index += 1;
            continue;
        }

        let inner = &message[start..end];
        let should_unquote = !inner.is_empty()
            && !inner.chars().any(char::is_whitespace)
            && inner
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | ':'));

        if should_unquote {
            output.push_str(inner);
        } else {
            output.push('\'');
            output.push_str(inner);
            output.push('\'');
        }
        index = end + 1;
    }

    output
}

fn uppercase_first_message_letter(mut value: String) -> String {
    let mut in_brackets = false;
    for (index, ch) in value.char_indices() {
        if ch == '[' {
            in_brackets = true;
            continue;
        }
        if ch == ']' {
            in_brackets = false;
            continue;
        }
        if in_brackets {
            continue;
        }
        if ch.is_ascii_alphabetic() {
            if ch.is_ascii_lowercase() {
                value.replace_range(
                    index..index + ch.len_utf8(),
                    &ch.to_ascii_uppercase().to_string(),
                );
            }
            break;
        }
    }
    value
}

fn colorize_message_tokens(message: &str) -> String {
    let mut rendered = String::new();
    let mut remaining = message;
    let mut token_index = 0_usize;

    while let Some(after_open) = remaining.strip_prefix('[') {
        let Some((token, tail)) = after_open.split_once(']') else {
            break;
        };

        let token_text = format!("[{token}]");
        let styled = match token_color(token) {
            TokenColor::Green => token_text.green().to_string(),
            TokenColor::Red => token_text.red().to_string(),
            TokenColor::Blue => token_text.blue().to_string(),
            TokenColor::Yellow => token_text.yellow().to_string(),
            TokenColor::Cyan => token_text.cyan().to_string(),
            TokenColor::Orange => token_text.truecolor(255, 143, 0).to_string(),
            TokenColor::BrandBlue => token_text.truecolor(52, 120, 246).to_string(),
            TokenColor::RedisRed => token_text.truecolor(220, 53, 69).to_string(),
            TokenColor::Purple => token_text.truecolor(146, 103, 196).to_string(),
            TokenColor::Gray => token_text.dimmed().to_string(),
            TokenColor::PostgresBlue => token_text.truecolor(51, 103, 145).to_string(),
            TokenColor::MysqlBlue => token_text.truecolor(0, 117, 143).to_string(),
            TokenColor::MariaBlue => token_text.truecolor(0, 130, 199).to_string(),
            TokenColor::MinioRed => token_text.truecolor(198, 40, 40).to_string(),
            TokenColor::RustOrange => token_text.truecolor(206, 121, 0).to_string(),
            TokenColor::SearchGreen => token_text.truecolor(46, 204, 113).to_string(),
            TokenColor::Teal => token_text.truecolor(0, 162, 255).to_string(),
        };
        rendered.push_str(&styled);
        if let Some(width) = TOKEN_COLUMN_WIDTHS.get(token_index) {
            let pad = width.saturating_sub(token_text.len()).max(1);
            if pad > 0 {
                rendered.push_str(&" ".repeat(pad));
            }
        }
        token_index += 1;

        remaining = tail.trim_start();
        if !remaining.is_empty() {
            continue;
        }

        break;
    }

    if rendered.is_empty() {
        return colorize_message_payload(message);
    }

    while token_index < TOKEN_COLUMN_WIDTHS.len() {
        rendered.push_str(&" ".repeat(TOKEN_COLUMN_WIDTHS[token_index]));
        token_index += 1;
    }

    if !remaining.is_empty() && !rendered.ends_with(' ') && !remaining.starts_with(' ') {
        rendered.push(' ');
    }
    rendered.push_str(&colorize_message_payload(remaining));
    rendered
}

fn colorize_message_payload(message: &str) -> String {
    let mut rendered = String::new();
    let mut index = 0_usize;
    let bytes = message.as_bytes();

    while index < bytes.len() {
        if message[index..].starts_with("https://") || message[index..].starts_with("http://") {
            let end = scan_until_whitespace(message, index);
            let token = &message[index..end];
            rendered.push_str(&token.truecolor(80, 220, 255).underline().to_string());
            index = end;
            continue;
        }

        if bytes[index] == b'/'
            && (index == 0
                || bytes[index - 1].is_ascii_whitespace()
                || matches!(bytes[index - 1], b'\'' | b'"' | b'('))
        {
            let end = scan_until_whitespace(message, index);
            let token = &message[index..end];
            rendered.push_str(&token.truecolor(120, 170, 255).to_string());
            index = end;
            continue;
        }

        if bytes[index] == b'\'' {
            let start = index;
            index += 1;
            while index < bytes.len() && bytes[index] != b'\'' {
                index += 1;
            }
            if index < bytes.len() {
                index += 1;
            }
            let token = &message[start..index];
            rendered.push_str(&token.truecolor(255, 200, 87).to_string());
            continue;
        }

        if bytes[index] == b'`' {
            let start = index + 1;
            let mut end = start;
            while end < bytes.len() && bytes[end] != b'`' {
                end += 1;
            }

            if end < bytes.len() {
                let token = &message[start..end];
                rendered.push_str(&token.truecolor(255, 200, 87).to_string());
                index = end + 1;
                continue;
            }
        }

        if let Some((end, tone)) = summary_count_span(message, index) {
            let token = &message[index..end];
            let styled = match tone {
                SummaryTone::Success => token.green().to_string(),
                SummaryTone::Failure => token.red().to_string(),
                SummaryTone::Skipped => token.yellow().to_string(),
            };
            rendered.push_str(&styled);
            index = end;
            continue;
        }

        if bytes[index].is_ascii_digit() {
            let start = index;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            let end = index;
            let len = end - start;
            let prev_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
            let next_ok = end >= bytes.len() || !bytes[end].is_ascii_alphanumeric();
            let token = &message[start..end];
            if prev_ok && next_ok && (2..=5).contains(&len) {
                rendered.push_str(&token.truecolor(255, 120, 220).to_string());
            } else {
                rendered.push_str(token);
            }
            continue;
        }

        if is_token_start(bytes, index) {
            let end = scan_until_whitespace(message, index);
            let token = &message[index..end];
            if is_image_like_token(token) || token.starts_with("acme-") {
                rendered.push_str(&token.truecolor(255, 200, 87).to_string());
                index = end;
                continue;
            }
        }

        let ch = message[index..].chars().next().unwrap_or_default();
        rendered.push(ch);
        index += ch.len_utf8();
    }

    rendered
}

#[derive(Clone, Copy)]
enum SummaryTone {
    Success,
    Failure,
    Skipped,
}

fn summary_count_span(message: &str, index: usize) -> Option<(usize, SummaryTone)> {
    let bytes = message.as_bytes();
    if index >= bytes.len() || !bytes[index].is_ascii_digit() {
        return None;
    }

    let mut end_num = index;
    while end_num < bytes.len() && bytes[end_num].is_ascii_digit() {
        end_num += 1;
    }
    if end_num >= bytes.len() || bytes[end_num] != b' ' {
        return None;
    }

    let after_space = end_num + 1;
    for (word, tone) in [
        ("succeeded", SummaryTone::Success),
        ("failed", SummaryTone::Failure),
        ("skipped", SummaryTone::Skipped),
    ] {
        if message[after_space..].starts_with(word) {
            return Some((after_space + word.len(), tone));
        }
    }

    None
}

/// Returns whether this character can start a token.
fn is_token_start(bytes: &[u8], index: usize) -> bool {
    index == 0 || bytes[index - 1].is_ascii_whitespace() || bytes[index - 1] == b'('
}

/// Returns whether a token looks like a container image reference.
fn is_image_like_token(token: &str) -> bool {
    token.contains('/')
        && !token.starts_with("http://")
        && !token.starts_with("https://")
        && !token.starts_with('/')
        && token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | ':'))
}

fn scan_until_whitespace(message: &str, start: usize) -> usize {
    let bytes = message.as_bytes();
    let mut index = start;
    while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    index
}

fn strip_ansi_codes(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0_usize;

    while index < bytes.len() {
        if bytes[index] == 0x1b && index + 1 < bytes.len() && bytes[index + 1] == b'[' {
            index += 2;
            while index < bytes.len() {
                let byte = bytes[index];
                if byte.is_ascii_alphabetic() {
                    index += 1;
                    break;
                }
                index += 1;
            }
            continue;
        }

        output.push(char::from(bytes[index]));
        index += 1;
    }

    output
}

#[derive(Clone, Copy)]
enum TokenColor {
    Green,
    Red,
    Blue,
    Yellow,
    Cyan,
    Orange,
    BrandBlue,
    RedisRed,
    Purple,
    Gray,
    PostgresBlue,
    MysqlBlue,
    MariaBlue,
    MinioRed,
    RustOrange,
    SearchGreen,
    Teal,
}

fn token_color(token: &str) -> TokenColor {
    match token {
        "out" => TokenColor::Green,
        "err" => TokenColor::Red,
        "api" => TokenColor::Blue,
        "app" | "laravel" | "frankenphp" => TokenColor::Orange,
        "mongodb" | "mongo" => TokenColor::Green,
        "postgres" | "pg" | "database" | "db" => TokenColor::PostgresBlue,
        "mysql" => TokenColor::MysqlBlue,
        "mariadb" => TokenColor::MariaBlue,
        "redis" | "valkey" | "memcached" | "cache" => TokenColor::RedisRed,
        "minio" => TokenColor::MinioRed,
        "rustfs" | "object-store" | "object_store" => TokenColor::RustOrange,
        "meilisearch" => TokenColor::SearchGreen,
        "typesense" | "search" => TokenColor::Teal,
        "gotenberg" => TokenColor::BrandBlue,
        "mailhog" | "mailpit" | "dusk" | "selenium" | "rabbitmq" | "soketi" | "scheduler" => {
            TokenColor::Purple
        }
        "caddy" => TokenColor::Yellow,
        "swarm" => TokenColor::Gray,
        _ => TokenColor::Cyan,
    }
}

#[cfg(test)]
mod tests;
