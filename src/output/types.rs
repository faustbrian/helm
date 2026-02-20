//! output logger types and level/persistence mappings.

use colored::Colorize;
use tracing::Level;

const LEVEL_COLUMN_WIDTH: usize = 15;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Persistence {
    Transient,
    Persistent,
}

impl Persistence {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Transient => "transient",
            Self::Persistent => "persistent",
        }
    }

    pub(super) fn from_str(value: &str) -> Option<Self> {
        match value {
            "transient" => Some(Self::Transient),
            "persistent" => Some(Self::Persistent),
            _ => None,
        }
    }
}

impl LogLevel {
    pub(super) const fn as_laravel(self) -> &'static str {
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

    pub(super) const fn as_tracing_level(self) -> Level {
        match self {
            Self::Emergency | Self::Alert | Self::Critical | Self::Error => Level::ERROR,
            Self::Warn => Level::WARN,
            Self::Notice | Self::Info | Self::Success => Level::INFO,
            Self::Debug => Level::DEBUG,
        }
    }

    pub(super) const fn from_tracing(level: Level) -> Self {
        match level {
            Level::ERROR => Self::Error,
            Level::WARN => Self::Warn,
            Level::INFO => Self::Info,
            Level::DEBUG | Level::TRACE => Self::Debug,
        }
    }

    pub(super) fn from_laravel(value: &str) -> Option<Self> {
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

    pub(super) const fn visible_when_quiet(self) -> bool {
        matches!(
            self,
            Self::Emergency | Self::Alert | Self::Critical | Self::Error | Self::Warn
        )
    }

    pub(super) fn padded_laravel_label(self) -> String {
        let level_width = LEVEL_COLUMN_WIDTH.saturating_sub(6);
        let level = self.as_laravel();
        let dots = ".".repeat(level_width.saturating_sub(level.len()));
        format!("local.{level}{dots}")
    }

    pub(super) fn colorize_laravel_label(self, token: &str) -> String {
        apply_laravel_label_color(self, token)
    }
}

fn apply_laravel_label_color(level: LogLevel, token: &str) -> String {
    match level {
        LogLevel::Emergency | LogLevel::Alert | LogLevel::Critical | LogLevel::Error => {
            token.red().to_string()
        }
        LogLevel::Warn => token.yellow().to_string(),
        LogLevel::Notice | LogLevel::Info => token.blue().to_string(),
        LogLevel::Debug => token.dimmed().to_string(),
        LogLevel::Success => token.green().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistence_converts_to_and_from_strings() {
        assert_eq!(Persistence::Transient.as_str(), "transient");
        assert_eq!(Persistence::Persistent.as_str(), "persistent");
        assert_eq!(
            Persistence::from_str("transient"),
            Some(Persistence::Transient)
        );
        assert_eq!(
            Persistence::from_str("persistent"),
            Some(Persistence::Persistent)
        );
        assert_eq!(Persistence::from_str("read-write"), None);
    }

    #[test]
    fn loglevel_maps_to_laravel_strings_and_tracing_levels() {
        assert_eq!(LogLevel::Error.as_laravel(), "ERROR");
        assert_eq!(LogLevel::Warn.as_laravel(), "WARNING");
        assert_eq!(LogLevel::Success.as_laravel(), "SUCCESS");
        assert_eq!(LogLevel::Emergency.as_tracing_level(), Level::ERROR);
        assert_eq!(LogLevel::Warn.as_tracing_level(), Level::WARN);
        assert_eq!(LogLevel::Debug.as_tracing_level(), Level::DEBUG);
        assert_eq!(LogLevel::from_tracing(Level::ERROR), LogLevel::Error);
        assert_eq!(LogLevel::from_tracing(Level::TRACE), LogLevel::Debug);
        assert_eq!(LogLevel::from_laravel("NOTICE"), Some(LogLevel::Notice));
        assert_eq!(LogLevel::from_laravel("INFO"), Some(LogLevel::Info));
        assert_eq!(LogLevel::from_laravel("UNKNOWN"), None);
    }

    #[test]
    fn loglevel_visibility_and_padding_behave_as_expected() {
        assert!(LogLevel::Error.visible_when_quiet());
        assert!(LogLevel::Warn.visible_when_quiet());
        assert!(!LogLevel::Info.visible_when_quiet());
        assert!(!LogLevel::Debug.visible_when_quiet());

        let padded = LogLevel::Error.padded_laravel_label();
        assert!(padded.starts_with("local.ERROR"));
        assert_eq!(padded.len(), 15);
    }

    #[test]
    fn loglevel_colorization_uses_expected_color_paths() {
        let source = "local.INFO";
        colored::control::set_override(true);
        let error = LogLevel::Error.colorize_laravel_label(source);
        let notice = LogLevel::Notice.colorize_laravel_label(source);
        let warn = LogLevel::Warn.colorize_laravel_label(source);
        colored::control::set_override(false);

        assert!(error.starts_with('\x1b'));
        assert_ne!(error, source.to_owned());
        assert_ne!(error, notice);
        assert_ne!(warn, notice);
    }
}
