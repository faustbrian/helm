use super::{
    CertificateTrustStore, HostCommand, HostCommandExecutor, LocalCaIdentity, TrustStoreError,
    require_host_command_success,
};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
const LOCAL_CA_DIRECTORY: &str = "/usr/local/share/ca-certificates";

/// Debian-family system trust adapter using `update-ca-certificates`.
pub(crate) struct DebianCertificateTrustStore<E> {
    executor: E,
    local_ca_directory: PathBuf,
}

impl<E> DebianCertificateTrustStore<E> {
    #[cfg(target_os = "linux")]
    pub(crate) fn new(executor: E) -> Self {
        Self::with_local_ca_directory(executor, LOCAL_CA_DIRECTORY)
    }

    pub(crate) fn with_local_ca_directory(
        executor: E,
        local_ca_directory: impl Into<PathBuf>,
    ) -> Self {
        Self {
            executor,
            local_ca_directory: local_ca_directory.into(),
        }
    }

    pub(crate) fn managed_certificate_path(&self, identity: &LocalCaIdentity) -> PathBuf {
        self.local_ca_directory.join(format!(
            "stackctl-{}.crt",
            identity.sha256_hex().to_ascii_lowercase()
        ))
    }
}

impl<E: HostCommandExecutor> DebianCertificateTrustStore<E> {
    fn install_managed_certificate(
        &self,
        source: &str,
        target: &str,
    ) -> Result<(), TrustStoreError> {
        let output = self.executor.execute(&HostCommand::new(
            "sudo",
            ["install", "-m", "0644", source, target],
        ))?;
        require_host_command_success("copy Stackctl CA into Debian local roots", &output)
    }

    fn remove_managed_certificate(&self, target: &str) -> Result<(), TrustStoreError> {
        let output = self
            .executor
            .execute(&HostCommand::new("sudo", ["rm", "-f", target]))?;
        require_host_command_success("remove Stackctl CA from Debian local roots", &output)
    }

    fn refresh_system_trust(&self, fresh: bool) -> Result<(), TrustStoreError> {
        let arguments = if fresh {
            vec!["update-ca-certificates", "--fresh"]
        } else {
            vec!["update-ca-certificates"]
        };
        let output = self
            .executor
            .execute(&HostCommand::new("sudo", arguments))?;
        require_host_command_success("update Debian system CA certificates", &output)
    }
}

impl<E: HostCommandExecutor> CertificateTrustStore for DebianCertificateTrustStore<E> {
    fn contains(
        &self,
        identity: &LocalCaIdentity,
        _certificate_path: &Path,
    ) -> Result<bool, TrustStoreError> {
        let managed_path = self.managed_certificate_path(identity);
        let certificate = match std::fs::read_to_string(&managed_path) {
            Ok(certificate) => certificate,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
            Err(error) => {
                return Err(TrustStoreError::new(format!(
                    "failed to read managed Debian CA '{}': {error}",
                    managed_path.display()
                )));
            }
        };
        let observed = LocalCaIdentity::from_pem(&certificate).map_err(|error| {
            TrustStoreError::new(format!(
                "failed to validate managed Debian CA '{}': {error}",
                managed_path.display()
            ))
        })?;

        if observed != *identity {
            return Err(TrustStoreError::new(format!(
                "managed Debian CA '{}' does not match expected fingerprint {}",
                managed_path.display(),
                identity.sha256_hex()
            )));
        }

        Ok(true)
    }

    fn install(
        &self,
        identity: &LocalCaIdentity,
        certificate_path: &Path,
    ) -> Result<(), TrustStoreError> {
        let source = utf8_path("Debian CA source", certificate_path)?;
        let managed_path = self.managed_certificate_path(identity);
        let target = utf8_path("managed Debian CA", &managed_path)?;
        self.install_managed_certificate(source, target)?;
        if let Err(failure) = self.refresh_system_trust(false) {
            let rollback = self
                .remove_managed_certificate(target)
                .and_then(|()| self.refresh_system_trust(true));
            if let Err(rollback) = rollback {
                return Err(TrustStoreError::new(format!(
                    "{failure}; failed to roll back managed Debian CA: {rollback}"
                )));
            }

            return Err(failure);
        }

        Ok(())
    }

    fn remove(
        &self,
        identity: &LocalCaIdentity,
        certificate_path: &Path,
    ) -> Result<(), TrustStoreError> {
        let source = utf8_path("Debian CA source", certificate_path)?;
        let managed_path = self.managed_certificate_path(identity);
        let target = utf8_path("managed Debian CA", &managed_path)?;
        self.remove_managed_certificate(target)?;
        if let Err(failure) = self.refresh_system_trust(true) {
            let rollback = self
                .install_managed_certificate(source, target)
                .and_then(|()| self.refresh_system_trust(false));
            if let Err(rollback) = rollback {
                return Err(TrustStoreError::new(format!(
                    "{failure}; failed to restore managed Debian CA: {rollback}"
                )));
            }

            return Err(failure);
        }

        Ok(())
    }
}

fn utf8_path<'a>(field: &str, path: &'a Path) -> Result<&'a str, TrustStoreError> {
    path.to_str()
        .ok_or_else(|| TrustStoreError::new(format!("{field} path must contain valid UTF-8")))
}
