use super::{
    CertificateTrustStore, HostCommand, HostCommandExecutor, LocalCaIdentity, TrustStoreError,
    require_host_command_success,
};
use std::path::Path;

/// macOS per-user trust adapter for one exact Stackctl CA.
pub(crate) struct MacOsCertificateTrustStore<E> {
    executor: E,
}

impl<E> MacOsCertificateTrustStore<E> {
    pub(crate) const fn new(executor: E) -> Self {
        Self { executor }
    }
}

impl<E: HostCommandExecutor> CertificateTrustStore for MacOsCertificateTrustStore<E> {
    fn contains(
        &self,
        _identity: &LocalCaIdentity,
        certificate_path: &Path,
    ) -> Result<bool, TrustStoreError> {
        let certificate_path = certificate_path.to_str().ok_or_else(|| {
            TrustStoreError::new("macOS CA certificate path must contain valid UTF-8")
        })?;
        let output = self.executor.execute(&HostCommand::new(
            "security",
            [
                "verify-cert",
                "-c",
                certificate_path,
                "-p",
                "basic",
                "-l",
                "-L",
                "-q",
            ],
        ))?;

        Ok(output.succeeded())
    }

    fn install(
        &self,
        _identity: &LocalCaIdentity,
        certificate_path: &Path,
    ) -> Result<(), TrustStoreError> {
        let certificate_path = certificate_path.to_str().ok_or_else(|| {
            TrustStoreError::new("macOS CA certificate path must contain valid UTF-8")
        })?;
        let output = self.executor.execute(&HostCommand::new(
            "security",
            ["add-trusted-cert", "-r", "trustRoot", certificate_path],
        ))?;

        require_host_command_success("install Stackctl CA in macOS user trust settings", &output)
    }

    fn remove(
        &self,
        _identity: &LocalCaIdentity,
        certificate_path: &Path,
    ) -> Result<(), TrustStoreError> {
        let certificate_path = certificate_path.to_str().ok_or_else(|| {
            TrustStoreError::new("macOS CA certificate path must contain valid UTF-8")
        })?;
        let output = self.executor.execute(&HostCommand::new(
            "security",
            ["remove-trusted-cert", certificate_path],
        ))?;

        require_host_command_success("remove Stackctl CA from macOS user trust settings", &output)
    }
}
