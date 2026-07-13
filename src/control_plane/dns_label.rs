use super::IdentityError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DnsLabel(String);

impl DnsLabel {
    pub(super) fn new(kind: &'static str, value: &str) -> Result<Self, IdentityError> {
        if is_valid(value) {
            return Ok(Self(value.to_owned()));
        }

        Err(IdentityError::InvalidName {
            kind,
            value: value.to_owned(),
        })
    }

    pub(super) fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_valid(value: &str) -> bool {
    let bytes = value.as_bytes();

    if bytes.is_empty() || bytes.len() > 63 {
        return false;
    }

    let Some(first) = bytes.first() else {
        return false;
    };
    let Some(last) = bytes.last() else {
        return false;
    };

    is_alphanumeric(*first)
        && is_alphanumeric(*last)
        && bytes
            .iter()
            .all(|byte| is_alphanumeric(*byte) || *byte == b'-')
}

fn is_alphanumeric(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit()
}
