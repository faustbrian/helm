//! `stackctl daemon trust` command handlers.

use crate::cli::args::{DaemonTrustArgs, DaemonTrustCommands};
use crate::control_plane::{
    CurrentCaTrustStatus, FilesystemCertificateStore, ProcessHostCommandExecutor, TrustChange,
    inspect_current_ca_trust, install_current_ca_trust, remove_current_ca_trust,
};
use crate::output::{self, LogLevel, Persistence};
use anyhow::{Result, bail};

pub(super) fn handle_daemon_trust(args: &DaemonTrustArgs) -> Result<()> {
    #[cfg(unix)]
    {
        let runtime_directory = crate::control_plane::default_unix_daemon_runtime_directory()?;
        let certificates = FilesystemCertificateStore::new(runtime_directory.join("tls"));

        #[cfg(target_os = "macos")]
        return handle_with_store(
            args.command,
            &certificates,
            &crate::control_plane::MacOsCertificateTrustStore::new(ProcessHostCommandExecutor),
        );

        #[cfg(target_os = "linux")]
        return handle_with_store(
            args.command,
            &certificates,
            &crate::control_plane::DebianCertificateTrustStore::new(ProcessHostCommandExecutor),
        );

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        bail!("singleton CA trust is not implemented for this Unix platform");
    }
}

/// Removes only the trust entry matching the persisted Stackctl CA.
#[cfg(unix)]
pub(super) fn remove_persisted_daemon_trust() -> Result<()> {
    let runtime_directory = crate::control_plane::default_unix_daemon_runtime_directory()?;
    let certificates = FilesystemCertificateStore::new(runtime_directory.join("tls"));

    #[cfg(target_os = "macos")]
    {
        drop(remove_current_ca_trust(
            &certificates,
            &crate::control_plane::MacOsCertificateTrustStore::new(ProcessHostCommandExecutor),
        )?);

        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        drop(remove_current_ca_trust(
            &certificates,
            &crate::control_plane::DebianCertificateTrustStore::new(ProcessHostCommandExecutor),
        )?);

        return Ok(());
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    bail!("singleton CA trust removal is not implemented for this Unix platform")
}

fn handle_with_store(
    command: DaemonTrustCommands,
    certificates: &FilesystemCertificateStore,
    trust_store: &impl crate::control_plane::CertificateTrustStore,
) -> Result<()> {
    match command {
        DaemonTrustCommands::Install => {
            let result = install_current_ca_trust(
                certificates,
                trust_store,
                time::OffsetDateTime::now_utc(),
            )?;
            let verb = match result.change() {
                TrustChange::Installed => "Installed",
                TrustChange::Unchanged => "Retained",
                TrustChange::Removed => unreachable!("install cannot remove trust"),
            };
            output::event(
                "daemon",
                LogLevel::Success,
                &format!(
                    "{verb} OS trust for Stackctl CA {}",
                    result.identity().sha256_hex()
                ),
                Persistence::Persistent,
            );
            Ok(())
        }
        DaemonTrustCommands::Status => match inspect_current_ca_trust(certificates, trust_store)? {
            CurrentCaTrustStatus::Trusted(identity) => {
                output::event(
                    "daemon",
                    LogLevel::Success,
                    &format!("Stackctl CA {} is trusted", identity.sha256_hex()),
                    Persistence::Persistent,
                );
                Ok(())
            }
            CurrentCaTrustStatus::Untrusted(identity) => bail!(
                "Stackctl CA {} is not trusted; run `stackctl daemon trust install`",
                identity.sha256_hex()
            ),
            CurrentCaTrustStatus::Absent => {
                bail!("Stackctl CA material does not exist; run `stackctl daemon trust install`")
            }
        },
        DaemonTrustCommands::Remove => {
            let Some(result) = remove_current_ca_trust(certificates, trust_store)? else {
                output::event(
                    "daemon",
                    LogLevel::Info,
                    "No persisted Stackctl CA exists, so no trust entry was removed",
                    Persistence::Persistent,
                );
                return Ok(());
            };
            let verb = match result.change() {
                TrustChange::Removed => "Removed",
                TrustChange::Unchanged => "Found no",
                TrustChange::Installed => unreachable!("removal cannot install trust"),
            };
            output::event(
                "daemon",
                LogLevel::Success,
                &format!(
                    "{verb} OS trust entry for Stackctl CA {}",
                    result.identity().sha256_hex()
                ),
                Persistence::Persistent,
            );
            Ok(())
        }
    }
}
