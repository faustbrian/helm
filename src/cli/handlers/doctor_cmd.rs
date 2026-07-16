use crate::cli::args::DoctorArgs;
use crate::control_plane::{
    CurrentCaTrustStatus, FilesystemCertificateStore, IpcOutcome, IpcPayload, IpcResult,
    ProcessHostCommandExecutor, inspect_current_ca_trust,
};
use crate::daemon::DaemonServiceStatus;
use crate::output::{self, LogLevel, Persistence};
use anyhow::{Result, bail};

pub(crate) fn handle_doctor(_args: &DoctorArgs) -> Result<()> {
    let service = crate::daemon::service_status()?;
    let mut issues = service_issues(&service);
    if issues.is_empty() {
        output::event(
            "doctor",
            LogLevel::Success,
            "Login service is installed, running, and IPC-responsive",
            Persistence::Persistent,
        );
        let response = super::daemon_cmd::send_singleton_request(IpcPayload::DaemonStatus)?;
        issues.extend(daemon_issues(response.outcome()));
        if issues.is_empty() {
            output::event(
                "doctor",
                LogLevel::Success,
                "Discovery is complete and the Engine is available and converged",
                Persistence::Persistent,
            );
        }
    }
    match current_ca_trust()? {
        CurrentCaTrustStatus::Trusted(identity) => output::event(
            "doctor",
            LogLevel::Success,
            &format!("Stackctl CA {} is trusted", identity.sha256_hex()),
            Persistence::Persistent,
        ),
        CurrentCaTrustStatus::Untrusted(identity) => issues.push(format!(
            "Stackctl CA {} is not trusted by the operating system",
            identity.sha256_hex()
        )),
        CurrentCaTrustStatus::Absent => {
            issues.push("Stackctl CA material is absent".to_owned());
        }
    }
    if issues.is_empty() {
        output::event(
            "doctor",
            LogLevel::Success,
            "Stackctl is healthy; no operator action is required",
            Persistence::Persistent,
        );
        return Ok(());
    }
    for issue in &issues {
        output::event("doctor", LogLevel::Error, issue, Persistence::Persistent);
    }

    bail!(
        "Stackctl doctor found {} issue(s); the daemon will continue self-healing retryable states",
        issues.len()
    )
}

fn service_issues(status: &DaemonServiceStatus) -> Vec<String> {
    if !status.installed {
        return vec![format!(
            "login service '{}' is not installed at {}",
            status.label,
            status.path.display()
        )];
    }
    if !status.running {
        return vec![format!(
            "login service '{}' is installed but not running",
            status.label
        )];
    }
    if !status.responsive {
        return vec![format!(
            "login service '{}' is running but not IPC-responsive",
            status.label
        )];
    }

    Vec::new()
}

fn daemon_issues(outcome: &IpcOutcome) -> Vec<String> {
    match outcome {
        IpcOutcome::Success {
            result:
                IpcResult::DaemonStatus {
                    discovery_complete,
                    engine_available,
                    engine_converged,
                    discovery_diagnostics,
                    reconciliation_diagnostic,
                },
        } => {
            let mut issues = Vec::new();
            if !discovery_complete {
                issues.push("project discovery has not completed".to_owned());
            }
            if !engine_available {
                issues.push("the selected Docker Engine is unavailable".to_owned());
            }
            if !engine_converged {
                issues.push("Engine reconciliation has not converged".to_owned());
            }
            issues.extend(
                discovery_diagnostics
                    .iter()
                    .map(|diagnostic| format!("{}: {}", diagnostic.code(), diagnostic.message())),
            );
            if let Some(diagnostic) = reconciliation_diagnostic {
                issues.push(format!("{}: {}", diagnostic.code(), diagnostic.message()));
            }
            issues
        }
        IpcOutcome::Failure { diagnostics } => diagnostics
            .iter()
            .map(|diagnostic| format!("{}: {}", diagnostic.code(), diagnostic.message()))
            .collect(),
        outcome => vec![format!(
            "daemon returned an unexpected doctor response: {outcome:?}"
        )],
    }
}

fn current_ca_trust() -> Result<CurrentCaTrustStatus> {
    let runtime_directory = crate::control_plane::default_unix_daemon_runtime_directory()?;
    let certificates = FilesystemCertificateStore::new(runtime_directory.join("tls"));

    #[cfg(target_os = "macos")]
    return inspect_current_ca_trust(
        &certificates,
        &crate::control_plane::MacOsCertificateTrustStore::new(ProcessHostCommandExecutor),
    )
    .map_err(Into::into);

    #[cfg(target_os = "linux")]
    return inspect_current_ca_trust(
        &certificates,
        &crate::control_plane::DebianCertificateTrustStore::new(ProcessHostCommandExecutor),
    )
    .map_err(Into::into);
}

#[cfg(test)]
mod tests {
    use super::daemon_issues;
    use crate::control_plane::{IpcDiagnostic, IpcOutcome, IpcResult};

    #[test]
    fn doctor_reports_all_daemon_failures_together() {
        let outcome = IpcOutcome::Success {
            result: IpcResult::DaemonStatus {
                discovery_complete: false,
                engine_available: false,
                engine_converged: false,
                discovery_diagnostics: vec![IpcDiagnostic::new(
                    "invalid_project",
                    "one project is invalid",
                    false,
                )],
                reconciliation_diagnostic: Some(IpcDiagnostic::new(
                    "engine_failed",
                    "reconciliation failed",
                    true,
                )),
            },
        };

        assert_eq!(
            daemon_issues(&outcome),
            [
                "project discovery has not completed",
                "the selected Docker Engine is unavailable",
                "Engine reconciliation has not converged",
                "invalid_project: one project is invalid",
                "engine_failed: reconciliation failed",
            ]
        );
    }
}
