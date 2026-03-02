//! Timestamp token parsing helpers for output normalization.

/// Returns whether a log line begins with a Laravel-style timestamp.
pub(in crate::output) fn is_laravel_timestamp(value: &str) -> bool {
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
pub(in crate::output) fn is_fractional_unix_timestamp(value: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::{is_fractional_unix_timestamp, is_laravel_timestamp};

    #[test]
    fn parse_laravel_timestamp_formats() {
        assert!(is_laravel_timestamp("2025-01-02 03:04:05"));
        assert!(!is_laravel_timestamp("2025-1-2 3:4:05"));
        assert!(!is_laravel_timestamp("abc"));
    }

    #[test]
    fn parse_fractional_unix_timestamp() {
        assert!(is_fractional_unix_timestamp("1680000000.123"));
        assert!(!is_fractional_unix_timestamp("1680000000"));
        assert!(!is_fractional_unix_timestamp("1680000000.123.4"));
        assert!(!is_fractional_unix_timestamp("foo.bar"));
    }
}
