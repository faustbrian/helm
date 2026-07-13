use super::{CredentialEntropy, CredentialGenerationError, CredentialSecret};

const SECRET_BYTES: usize = 32;

/// Generates one 256-bit credential without shell or external tooling.
pub(crate) fn generate_credential_secret(
    entropy: &impl CredentialEntropy,
) -> Result<CredentialSecret, CredentialGenerationError> {
    let mut bytes = [0_u8; SECRET_BYTES];
    entropy.fill(&mut bytes)?;

    Ok(CredentialSecret::new(hex::encode(bytes)))
}
