const SHARED_CONTAINER_PREFIX: &str = "stackctl-shared-";
const SHARED_IDENTITY_HEX_LENGTH: usize = 40;

/// Produces one deterministic Engine and DNS-safe name from a full fingerprint.
pub(crate) fn shared_container_name(identity: &str) -> String {
    debug_assert!(
        identity.len() >= SHARED_IDENTITY_HEX_LENGTH,
        "shared compatibility identity must contain at least 160 bits"
    );
    let identity = identity
        .chars()
        .take(SHARED_IDENTITY_HEX_LENGTH)
        .collect::<String>();

    format!("{SHARED_CONTAINER_PREFIX}{identity}")
}
