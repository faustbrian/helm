use super::{
    CertificateTrustStore, HostCommand, HostCommandExecutor, LocalCaIdentity, TrustStoreError,
    require_host_command_success,
};
use std::path::{Path, PathBuf};

/// macOS per-user trust adapter for one exact Stackctl CA.
pub(crate) struct MacOsCertificateTrustStore<E> {
    executor: E,
    user_keychain: Option<PathBuf>,
}

impl<E> MacOsCertificateTrustStore<E> {
    #[cfg(target_os = "macos")]
    pub(crate) fn new(executor: E) -> Self {
        let user_keychain = std::env::var_os("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library")
                .join("Keychains")
                .join("login.keychain-db")
        });

        Self {
            executor,
            user_keychain,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_user_keychain(executor: E, user_keychain: impl AsRef<Path>) -> Self {
        Self {
            executor,
            user_keychain: Some(user_keychain.as_ref().to_path_buf()),
        }
    }

    fn user_keychain(&self) -> Result<&str, TrustStoreError> {
        self.user_keychain
            .as_deref()
            .and_then(Path::to_str)
            .ok_or_else(|| {
                TrustStoreError::new(
                    "macOS user Keychain path requires an absolute UTF-8 HOME directory",
                )
            })
    }
}

impl<E: HostCommandExecutor> CertificateTrustStore for MacOsCertificateTrustStore<E> {
    fn contains(
        &self,
        identity: &LocalCaIdentity,
        certificate_path: &Path,
    ) -> Result<bool, TrustStoreError> {
        let user_keychain = self.user_keychain()?;
        let installed = self.executor.execute(&HostCommand::new(
            "security",
            ["find-certificate", "-a", "-Z", user_keychain],
        ))?;
        if !installed.succeeded()
            || !installed.stdout().lines().any(|line| {
                line.trim() == format!("SHA-256 hash: {}", identity.sha256_hex()).as_str()
            })
        {
            return Ok(false);
        }
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
        let user_keychain = self.user_keychain()?;
        let certificate_path = certificate_path.to_str().ok_or_else(|| {
            TrustStoreError::new("macOS CA certificate path must contain valid UTF-8")
        })?;
        let output = self.executor.execute(&HostCommand::new(
            "security",
            [
                "add-trusted-cert",
                "-r",
                "trustRoot",
                "-k",
                user_keychain,
                certificate_path,
            ],
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
