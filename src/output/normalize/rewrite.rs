//! Message rewrite helpers for output normalization.

use crate::output::LogLevel;

pub(super) fn rewrite_message_for_level(level: LogLevel, message: String) -> String {
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

    uppercase_first_message_letter(value)
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

#[cfg(test)]
mod tests {
    use super::{rewrite_message_for_level, uppercase_first_message_letter};
    use crate::output::LogLevel;

    #[test]
    fn rewrite_message_for_level_prefixes_errors() {
        assert_eq!(
            rewrite_message_for_level(LogLevel::Error, "Error: connection failed".to_owned()),
            "Operation failed: connection failed"
        );
        assert_eq!(
            rewrite_message_for_level(LogLevel::Error, "Failed to start: bad".to_owned()),
            "Start failed: bad".to_string()
        );
        assert_eq!(
            rewrite_message_for_level(LogLevel::Info, "Skipping this action".to_owned()),
            "Skipped this action".to_string()
        );
    }

    #[test]
    fn uppercase_first_message_letter_only_changes_first_word() {
        assert_eq!(
            uppercase_first_message_letter("[tag]message".to_owned()),
            "[tag]Message".to_owned()
        );
        assert_eq!(
            uppercase_first_message_letter("already".to_owned()),
            "Already".to_owned()
        );
    }
}
