/// Reports whether a key can be encoded in an Engine environment entry.
pub(super) fn is_valid_container_environment_key(key: &str) -> bool {
    !key.is_empty() && !key.contains(['=', '\0'])
}
