use super::CredentialGenerationError;

/// Replaceable cryptographic entropy source for managed credentials.
pub(crate) trait CredentialEntropy {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), CredentialGenerationError>;
}
