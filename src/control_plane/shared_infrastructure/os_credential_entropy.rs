use super::{CredentialEntropy, CredentialGenerationError};

/// Operating-system cryptographic random source for managed credentials.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct OsCredentialEntropy;

impl CredentialEntropy for OsCredentialEntropy {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), CredentialGenerationError> {
        getrandom::fill(bytes).map_err(|error| {
            CredentialGenerationError::new(format!(
                "failed to obtain operating-system credential entropy: {error}"
            ))
        })
    }
}
