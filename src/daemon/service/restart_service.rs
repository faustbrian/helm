use super::{
    DaemonServiceInstallOptions, DaemonServiceStatus, ServiceManager, launchd_domain, manager_name,
    run_command, service_definition, service_is_running, set_test_service_running_state,
};
use anyhow::{Context, Result, bail};
use std::fs;
use std::io::ErrorKind;

/// Restarts the installed login-time daemon and verifies operational readiness.
pub(crate) fn restart_service() -> Result<DaemonServiceStatus> {
    #[cfg(test)]
    return restart_service_with_readiness(|| Ok(()));

    #[cfg(not(test))]
    restart_service_with_readiness(super::verify_daemon_service_readiness::verify)
}

pub(super) fn restart_service_with_readiness(
    verify_readiness: impl FnOnce() -> Result<()>,
) -> Result<DaemonServiceStatus> {
    let definition = service_definition(&DaemonServiceInstallOptions {
        watch_dirs: Vec::new(),
        interval_secs: 30,
    })?;
    match fs::symlink_metadata(&definition.path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {}
        Ok(_) => bail!(
            "refusing to restart {} service '{}' because definition '{}' is not a real file",
            manager_name(definition.manager),
            definition.label,
            definition.path.display()
        ),
        Err(error) if error.kind() == ErrorKind::NotFound => bail!(
            "{} daemon watch service '{}' is not installed at {}; run `stackctl daemon service install --dir <DIR>`",
            manager_name(definition.manager),
            definition.label,
            definition.path.display()
        ),
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to inspect daemon service definition {}",
                    definition.path.display()
                )
            });
        }
    }

    restart_manager_service(definition.manager, &definition.label)?;
    set_test_service_running_state(true);
    if !service_is_running(definition.manager, &definition.label)? {
        bail!(
            "{} did not keep '{}' running after restart",
            manager_name(definition.manager),
            definition.label
        );
    }
    verify_readiness().context("restarted daemon did not become operationally ready")?;

    Ok(DaemonServiceStatus {
        manager: definition.manager,
        label: definition.label,
        path: definition.path,
        installed: true,
        running: true,
        responsive: true,
    })
}

fn restart_manager_service(manager: ServiceManager, label: &str) -> Result<()> {
    match manager {
        ServiceManager::Launchd => run_command(
            "launchctl",
            &[
                "kickstart".to_owned(),
                "-k".to_owned(),
                format!("{}/{}", launchd_domain()?, label),
            ],
            false,
        ),
        ServiceManager::SystemdUser => run_command(
            "systemctl",
            &["--user".to_owned(), "restart".to_owned(), label.to_owned()],
            false,
        ),
    }
}
