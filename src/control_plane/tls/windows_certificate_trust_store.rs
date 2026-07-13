use super::{
    CertificateTrustStore, HostCommand, HostCommandExecutor, LocalCaIdentity, TrustStoreError,
    require_host_command_success,
};
use std::path::Path;

const CERTIFICATE_NOT_FOUND: &str = "0x80092004";

/// Windows Current User root-store adapter for one exact Stackctl CA.
pub(crate) struct WindowsCertificateTrustStore<E> {
    executor: E,
}

impl<E> WindowsCertificateTrustStore<E> {
    pub(crate) const fn new(executor: E) -> Self {
        Self { executor }
    }
}

impl<E: HostCommandExecutor> CertificateTrustStore for WindowsCertificateTrustStore<E> {
    fn contains(&self, identity: &LocalCaIdentity) -> Result<bool, TrustStoreError> {
        let output = self.executor.execute(&HostCommand::new(
            "certutil",
            ["-user", "-store", "Root", identity.sha1_hex()],
        ))?;

        if output.succeeded() {
            return Ok(true);
        }
        if output.contains(CERTIFICATE_NOT_FOUND) {
            return Ok(false);
        }

        require_host_command_success("inspect Windows Current User root store", &output)?;

        Ok(false)
    }

    fn install(
        &self,
        _identity: &LocalCaIdentity,
        certificate_path: &Path,
    ) -> Result<(), TrustStoreError> {
        let certificate_path = certificate_path.to_str().ok_or_else(|| {
            TrustStoreError::new("Windows CA certificate path must contain valid UTF-8")
        })?;
        let output = self.executor.execute(&HostCommand::new(
            "certutil",
            ["-user", "-addstore", "Root", certificate_path],
        ))?;

        require_host_command_success("install Stackctl CA in Windows Current User roots", &output)
    }

    fn remove(&self, identity: &LocalCaIdentity) -> Result<(), TrustStoreError> {
        let output = self.executor.execute(&HostCommand::new(
            "certutil",
            ["-user", "-delstore", "Root", identity.sha1_hex()],
        ))?;

        require_host_command_success(
            "remove Stackctl CA from Windows Current User roots",
            &output,
        )
    }
}
