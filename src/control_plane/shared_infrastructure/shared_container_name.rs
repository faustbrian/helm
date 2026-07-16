use super::shared_identity_hex;

const SHARED_CONTAINER_PREFIX: &str = "stackctl-shared-";

/// Produces one deterministic Engine and DNS-safe name from a full fingerprint.
pub(crate) fn shared_container_name(identity: &str) -> String {
    let identity = shared_identity_hex(identity);

    format!("{SHARED_CONTAINER_PREFIX}{identity}")
}
