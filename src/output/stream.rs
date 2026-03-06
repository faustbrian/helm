//! Stream message normalization helpers.

use crate::output::LogLevel;
use crate::output::normalize::{
    normalize_log_message, strip_ansi_codes, strip_leading_bracket_timestamp,
    strip_leading_laravel_prefix,
};

pub(super) fn prepare_stream_message(message: &str) -> (LogLevel, String) {
    let normalized = strip_leading_bracket_timestamp(message);
    let sanitized = strip_ansi_codes(normalized);
    let (level, stripped_message) = derive_level_and_message(&sanitized);
    let standardized = normalize_log_message(level, stripped_message);
    (level, standardized)
}

fn derive_level_and_message(sanitized_message: &str) -> (LogLevel, &str) {
    let (embedded_level, stripped_message) = strip_leading_laravel_prefix(sanitized_message);
    (embedded_level.unwrap_or(LogLevel::Info), stripped_message)
}

#[cfg(test)]
mod tests {
    use crate::output::LogLevel;

    use super::prepare_stream_message;

    #[test]
    fn prepare_stream_message_defaults_to_info_when_level_missing() {
        let (level, message) = prepare_stream_message("[2025-01-01 00:00:00] service started");
        assert!(matches!(level, LogLevel::Info));
        assert_eq!(message, "[2025-01-01 00:00:00] Service started");
    }

    #[test]
    fn prepare_stream_message_preserves_embedded_laravel_level() {
        let (level, message) =
            prepare_stream_message("[2025-01-01 00:00:00] local.SUCCESS..: all systems go");
        assert!(matches!(level, LogLevel::Success));
        assert_eq!(message, "All systems go");
    }
}
