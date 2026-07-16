const SHARED_IDENTITY_HEX_LENGTH: usize = 40;

/// Returns the stable 160-bit prefix used in Engine names and state paths.
pub(crate) fn shared_identity_hex(identity: &str) -> String {
    debug_assert!(
        identity.len() >= SHARED_IDENTITY_HEX_LENGTH,
        "shared compatibility identity must contain at least 160 bits"
    );

    identity.chars().take(SHARED_IDENTITY_HEX_LENGTH).collect()
}
