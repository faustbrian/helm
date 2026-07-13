//! `stackctl daemon service` command handlers.

use crate::cli::args::{
    DaemonServiceArgs, DaemonServiceCommands, DaemonServiceInstallArgs, DaemonServicePrintArgs,
    DaemonServiceUninstallArgs,
};
use crate::daemon::{self, DaemonServiceInstallOptions, ServiceManager};
use crate::output::{self, LogLevel, Persistence};
use anyhow::Result;
use std::io::Write;

pub(super) fn handle_daemon_service(args: &DaemonServiceArgs) -> Result<()> {
    match &args.command {
        DaemonServiceCommands::Install(install) => handle_install(install),
        DaemonServiceCommands::Status => handle_status(),
        DaemonServiceCommands::Print(print) => handle_print(print),
        DaemonServiceCommands::Uninstall(uninstall) => handle_uninstall(uninstall),
    }
}

fn handle_install(args: &DaemonServiceInstallArgs) -> Result<()> {
    let status = daemon::install_service(&install_options(args.dir.clone(), args.interval))?;
    output::event(
        "daemon",
        LogLevel::Success,
        &format!(
            "Installed {} daemon watch service {} at {}",
            manager_name(status.manager),
            status.label,
            status.path.display()
        ),
        Persistence::Persistent,
    );
    Ok(())
}

fn handle_status() -> Result<()> {
    let status = daemon::service_status()?;
    let message = if status.installed {
        format!(
            "{} daemon watch service {} installed at {}",
            manager_name(status.manager),
            status.label,
            status.path.display()
        )
    } else {
        format!(
            "No {} daemon watch service {} installed at {}",
            manager_name(status.manager),
            status.label,
            status.path.display()
        )
    };
    output::event("daemon", LogLevel::Info, &message, Persistence::Persistent);
    Ok(())
}

fn handle_print(args: &DaemonServicePrintArgs) -> Result<()> {
    let definition = daemon::print_service(&install_options(args.dir.clone(), args.interval))?;
    std::io::stdout().write_all(definition.contents.as_bytes())?;
    std::io::stdout().flush()?;
    Ok(())
}

fn handle_uninstall(args: &DaemonServiceUninstallArgs) -> Result<()> {
    validate_uninstall_mode(args)?;
    let status = daemon::uninstall_service()?;
    let message = if status.installed {
        format!(
            "Removed {} daemon watch service {} from {}; retained data and Engine resources were preserved",
            manager_name(status.manager),
            status.label,
            status.path.display()
        )
    } else {
        format!(
            "No {} daemon watch service {} was installed at {}; retained data and Engine resources were preserved",
            manager_name(status.manager),
            status.label,
            status.path.display()
        )
    };
    output::event(
        "daemon",
        LogLevel::Success,
        &message,
        Persistence::Persistent,
    );
    Ok(())
}

fn validate_uninstall_mode(args: &DaemonServiceUninstallArgs) -> Result<()> {
    if args.delete_data && !args.confirm_delete_data {
        anyhow::bail!("delete-data uninstall requires --confirm-delete-data");
    }
    if args.delete_data {
        anyhow::bail!(
            "delete-data uninstall is unavailable: prune every retained service through its \
             verified adapter first; Stackctl will not remove the daemon, backups, or Engine \
             resources while complete deletion coverage is unproven"
        );
    }
    Ok(())
}

fn install_options(dirs: Vec<std::path::PathBuf>, interval: u64) -> DaemonServiceInstallOptions {
    DaemonServiceInstallOptions {
        watch_dirs: dirs,
        interval_secs: interval,
    }
}

fn manager_name(manager: ServiceManager) -> &'static str {
    match manager {
        ServiceManager::Launchd => "launchd",
        ServiceManager::SystemdUser => "systemd --user",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_data_fails_closed_before_service_removal() {
        let error = validate_uninstall_mode(&DaemonServiceUninstallArgs {
            keep_data: false,
            delete_data: true,
            confirm_delete_data: true,
        })
        .expect_err("delete-data must remain unavailable");

        assert!(
            error
                .to_string()
                .contains("delete-data uninstall is unavailable")
        );
    }

    #[test]
    fn keep_data_is_the_default_uninstall_mode() {
        validate_uninstall_mode(&DaemonServiceUninstallArgs {
            keep_data: false,
            delete_data: false,
            confirm_delete_data: false,
        })
        .expect("default uninstall must preserve data");
    }
}
