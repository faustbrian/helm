//! ANSI escape stripping helpers.

pub(in crate::output) fn strip_ansi_codes(value: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::strip_ansi_codes;

    #[test]
    fn strip_ansi_codes_removes_escape_sequences() {
        assert_eq!(
            strip_ansi_codes("\u{1b}[31mred\u{1b}[0m"),
            "red".to_string()
        );
    }

    #[test]
    fn strip_ansi_codes_keeps_plain_text() {
        assert_eq!(strip_ansi_codes("plain"), "plain");
    }
}
