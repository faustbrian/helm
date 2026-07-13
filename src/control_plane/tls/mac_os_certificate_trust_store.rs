use super::{
    CertificateTrustStore, HostCommand, HostCommandExecutor, HostCommandOutput, LocalCaIdentity,
    TrustStoreError,
};
use std::path::Path;

const SYSTEM_KEYCHAIN: &str = "/Library/Keychains/System.keychain";

/// macOS System Keychain adapter for one exact Stackctl CA.
pub(crate) struct MacOsCertificateTrustStore<E> {
    executor: E,
}

impl<E> MacOsCertificateTrustStore<E> {
    pub(crate) const fn new(executor: E) -> Self {
        Self { executor }
    }
}

impl<E: HostCommandExecutor> CertificateTrustStore for MacOsCertificateTrustStore<E> {
    fn contains(&self, identity: &LocalCaIdentity) -> Result<bool, TrustStoreError> {
        let output = self.executor.execute(&HostCommand::new(
            "security",
            ["find-certificate", "-a", "-Z", SYSTEM_KEYCHAIN],
        ))?;
        require_success("inspect macOS System Keychain", &output)?;

        Ok(output.stdout().lines().any(|line| {
            line.strip_prefix("SHA-256 hash:")
                .is_some_and(|hash| hash.trim().eq_ignore_ascii_case(identity.sha256_hex()))
        }))
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
            "sudo",
            [
                "security",
                "add-trusted-cert",
                "-d",
                "-r",
                "trustRoot",
                "-k",
                SYSTEM_KEYCHAIN,
                certificate_path,
            ],
        ))?;

        require_success("install Stackctl CA in macOS System Keychain", &output)
    }

    fn remove(&self, identity: &LocalCaIdentity) -> Result<(), TrustStoreError> {
        let output = self.executor.execute(&HostCommand::new(
            "sudo",
            [
                "security",
                "delete-certificate",
                "-Z",
                identity.sha256_hex(),
                SYSTEM_KEYCHAIN,
            ],
        ))?;

        require_success("remove Stackctl CA from macOS System Keychain", &output)
    }
}

fn require_success(action: &str, output: &HostCommandOutput) -> Result<(), TrustStoreError> {
    if output.succeeded() {
        return Ok(());
    }

    let detail = output.stderr().trim();
    let suffix = if detail.is_empty() {
        String::new()
    } else {
        format!(": {detail}")
    };

    Err(TrustStoreError::new(format!("failed to {action}{suffix}")))
}
