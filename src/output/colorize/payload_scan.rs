//! Payload scanning helpers for output message colorization.

#[derive(Clone, Copy)]
pub(super) enum SummaryTone {
    Success,
    Failure,
    Skipped,
}

pub(super) fn summary_count_span(message: &str, index: usize) -> Option<(usize, SummaryTone)> {
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
pub(super) fn is_token_start(bytes: &[u8], index: usize) -> bool {
    index == 0 || bytes[index - 1].is_ascii_whitespace() || bytes[index - 1] == b'('
}

/// Returns whether a token looks like a container image reference.
pub(super) fn is_image_like_token(token: &str) -> bool {
    token.contains('/')
        && !token.starts_with("http://")
        && !token.starts_with("https://")
        && !token.starts_with('/')
        && token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | ':'))
}

pub(super) fn scan_until_whitespace(message: &str, start: usize) -> usize {
    let bytes = message.as_bytes();
    let mut index = start;
    while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    index
}
