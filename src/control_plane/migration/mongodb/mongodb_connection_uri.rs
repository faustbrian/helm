/// Builds an RFC 3986-safe MongoDB URI without exposing raw credentials.
pub(super) fn mongodb_connection_uri(
    username: &str,
    password: &str,
    database: &str,
    authentication_database: &str,
) -> String {
    let username = percent_encode(username);
    let password = percent_encode(password);
    let database = percent_encode(database);
    let authentication_database = percent_encode(authentication_database);

    format!(
        "mongodb://{username}:{password}@127.0.0.1:27017/{database}?authSource={authentication_database}"
    )
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());

    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }

    encoded
}

#[cfg(test)]
mod tests {
    use super::mongodb_connection_uri;

    #[test]
    fn encodes_every_reserved_and_non_ascii_credential_byte() {
        assert_eq!(
            mongodb_connection_uri("user name", "päss:/?#[]@", "stackctl-db", "stackctl-db"),
            "mongodb://user%20name:p%C3%A4ss%3A%2F%3F%23%5B%5D%40@127.0.0.1:27017/stackctl-db?authSource=stackctl-db"
        );
    }
}
