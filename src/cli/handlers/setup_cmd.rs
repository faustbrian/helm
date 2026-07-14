//! One-time v8 control-plane setup.

mod watched_roots;

use crate::cli::args::SetupArgs;
use crate::control_plane::{
    CertificateTrustStore, FilesystemCertificateStore, ProcessHostCommandExecutor,
    SystemLocalhostResolver, TrustChange, install_current_ca_trust, remove_current_ca_trust,
    verify_stackctl_localhost_resolution,
};
use crate::daemon::DaemonServiceInstallOptions;
use crate::output::{self, LogLevel, Persistence};
use anyhow::{Result, bail};
use time::OffsetDateTime;
use watched_roots::canonical_watched_roots;

pub(crate) fn handle_setup(args: &SetupArgs) -> Result<()> {
    let runtime_directory = crate::control_plane::default_unix_daemon_runtime_directory()?;
    let certificates = FilesystemCertificateStore::new(runtime_directory.join("tls"));

    #[cfg(target_os = "macos")]
    return handle_with_store(
        args,
        &certificates,
        &crate::control_plane::MacOsCertificateTrustStore::new(ProcessHostCommandExecutor),
    );

    #[cfg(target_os = "linux")]
    return handle_with_store(
        args,
        &certificates,
        &crate::control_plane::DebianCertificateTrustStore::new(ProcessHostCommandExecutor),
    );

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    bail!("Stackctl setup is not implemented for this Unix platform")
}

fn handle_with_store(
    args: &SetupArgs,
    certificates: &FilesystemCertificateStore,
    trust_store: &impl CertificateTrustStore,
) -> Result<()> {
    let mut operations = HostSetupOperations {
        args,
        certificates,
        trust_store,
        watched_roots: None,
    };
    execute_setup(&mut operations)?;
    output::event(
        "setup",
        LogLevel::Success,
        &format!(
            "Configured trusted HTTPS and started the login-time control plane for {} watched root(s)",
            args.dir.len()
        ),
        Persistence::Persistent,
    );

    Ok(())
}

trait SetupOperations {
    fn preflight(&mut self) -> Result<()>;

    fn install_trust(&mut self) -> Result<TrustChange>;

    fn install_service(&mut self) -> Result<()>;

    fn remove_trust(&mut self) -> Result<()>;
}

fn execute_setup(operations: &mut impl SetupOperations) -> Result<()> {
    operations.preflight()?;
    let trust_change = operations.install_trust()?;
    if let Err(service_error) = operations.install_service() {
        if trust_change == TrustChange::Installed
            && let Err(rollback_error) = operations.remove_trust()
        {
            bail!(
                "service installation failed: {service_error}; trust rollback failed: {rollback_error}"
            );
        }

        return Err(service_error);
    }

    Ok(())
}

struct HostSetupOperations<'a, Store> {
    args: &'a SetupArgs,
    certificates: &'a FilesystemCertificateStore,
    trust_store: &'a Store,
    watched_roots: Option<Vec<std::path::PathBuf>>,
}

impl<Store> SetupOperations for HostSetupOperations<'_, Store>
where
    Store: CertificateTrustStore,
{
    fn preflight(&mut self) -> Result<()> {
        self.watched_roots = Some(canonical_watched_roots(&self.args.dir)?);
        verify_stackctl_localhost_resolution(&SystemLocalhostResolver).map_err(Into::into)
    }

    fn install_trust(&mut self) -> Result<TrustChange> {
        install_current_ca_trust(
            self.certificates,
            self.trust_store,
            OffsetDateTime::now_utc(),
        )
        .map(|result| result.change())
        .map_err(Into::into)
    }

    fn install_service(&mut self) -> Result<()> {
        let watched_roots = self
            .watched_roots
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("setup preflight did not validate watched roots"))?;
        crate::daemon::install_service(&DaemonServiceInstallOptions {
            watch_dirs: watched_roots.clone(),
            interval_secs: self.args.interval,
        })
        .map(|_| ())
    }

    fn remove_trust(&mut self) -> Result<()> {
        remove_current_ca_trust(self.certificates, self.trust_store)
            .map(|_| ())
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::{SetupOperations, canonical_watched_roots, execute_setup};
    use crate::control_plane::TrustChange;
    use anyhow::{Result, bail};
    use std::path::PathBuf;

    #[test]
    fn setup_preflights_before_any_host_mutation() {
        let mut operations = RecordingSetupOperations::failing("preflight");

        let error = execute_setup(&mut operations).expect_err("failed preflight");

        assert_eq!(error.to_string(), "preflight failed");
        assert_eq!(operations.actions, ["preflight"]);
    }

    #[test]
    fn setup_removes_new_trust_when_service_installation_fails() {
        let mut operations = RecordingSetupOperations::failing("service");

        let error = execute_setup(&mut operations).expect_err("failed service");

        assert_eq!(error.to_string(), "service failed");
        assert_eq!(
            operations.actions,
            [
                "preflight",
                "install-trust",
                "install-service",
                "remove-trust"
            ]
        );
    }

    #[test]
    fn setup_preserves_preexisting_trust_when_service_installation_fails() {
        let mut operations = RecordingSetupOperations::failing("service");
        operations.trust_change = TrustChange::Unchanged;

        let error = execute_setup(&mut operations).expect_err("failed service");

        assert_eq!(error.to_string(), "service failed");
        assert_eq!(
            operations.actions,
            ["preflight", "install-trust", "install-service"]
        );
    }

    #[test]
    fn setup_reports_service_and_trust_rollback_failures_together() {
        let mut operations = RecordingSetupOperations::failing("service-and-rollback");

        let error = execute_setup(&mut operations).expect_err("failed transaction");

        assert_eq!(
            error.to_string(),
            "service installation failed: service failed; trust rollback failed: trust rollback failed"
        );
    }

    #[test]
    fn setup_rejects_missing_watched_roots_before_host_mutation() {
        let missing = PathBuf::from(format!(
            "/tmp/stackctl-missing-watched-root-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));

        let error = canonical_watched_roots(&[missing.clone()]).expect_err("missing root");

        assert!(error.to_string().contains("watched root"));
        assert!(error.to_string().contains(&missing.display().to_string()));
    }

    struct RecordingSetupOperations {
        actions: Vec<&'static str>,
        failure: &'static str,
        trust_change: TrustChange,
    }

    impl RecordingSetupOperations {
        fn failing(failure: &'static str) -> Self {
            Self {
                actions: Vec::new(),
                failure,
                trust_change: TrustChange::Installed,
            }
        }
    }

    impl SetupOperations for RecordingSetupOperations {
        fn preflight(&mut self) -> Result<()> {
            self.actions.push("preflight");
            if self.failure == "preflight" {
                bail!("preflight failed");
            }
            Ok(())
        }

        fn install_trust(&mut self) -> Result<TrustChange> {
            self.actions.push("install-trust");
            Ok(self.trust_change)
        }

        fn install_service(&mut self) -> Result<()> {
            self.actions.push("install-service");
            if matches!(self.failure, "service" | "service-and-rollback") {
                bail!("service failed");
            }
            Ok(())
        }

        fn remove_trust(&mut self) -> Result<()> {
            self.actions.push("remove-trust");
            if self.failure == "service-and-rollback" {
                bail!("trust rollback failed");
            }
            Ok(())
        }
    }
}
