//! `stackctl daemon service` command handlers.

use crate::cli::args::{
    DaemonServiceArgs, DaemonServiceCommands, DaemonServiceInstallArgs, DaemonServicePrintArgs,
    DaemonServiceRestartArgs, DaemonServiceUninstallArgs, SetupArgs,
};
use crate::daemon::{self, DaemonServiceInstallOptions, ServiceManager};
use crate::output::{self, LogLevel, Persistence};
use anyhow::Result;
use std::io::Write;

pub(super) fn handle_daemon_service(args: &DaemonServiceArgs) -> Result<()> {
    match &args.command {
        DaemonServiceCommands::Install(install) => handle_install(install),
        DaemonServiceCommands::Status => handle_status(),
        DaemonServiceCommands::Restart(restart) => handle_restart(restart),
        DaemonServiceCommands::Print(print) => handle_print(print),
        DaemonServiceCommands::Uninstall(uninstall) => handle_uninstall(uninstall),
    }
}

fn handle_restart(args: &DaemonServiceRestartArgs) -> Result<()> {
    if args.if_installed && !daemon::service_status()?.installed {
        return Ok(());
    }
    let status = daemon::restart_service()?;
    output::event(
        "daemon",
        LogLevel::Success,
        &format!(
            "Restarted {} daemon watch service {}; it is running and responsive from {}",
            manager_name(status.manager),
            status.label,
            status.path.display()
        ),
        Persistence::Persistent,
    );
    Ok(())
}

fn handle_install(args: &DaemonServiceInstallArgs) -> Result<()> {
    crate::cli::handlers::handle_setup(&setup_args(args))
}

fn setup_args(args: &DaemonServiceInstallArgs) -> SetupArgs {
    SetupArgs {
        dir: args.dir.clone(),
        interval: args.interval,
    }
}

fn handle_status() -> Result<()> {
    let status = daemon::service_status()?;
    let message = if status.responsive {
        format!(
            "{} daemon watch service {} is running and responsive from {}",
            manager_name(status.manager),
            status.label,
            status.path.display()
        )
    } else if status.running {
        anyhow::bail!(
            "{} daemon watch service {} is running from {} but is not responding to IPC; rerun `stackctl setup --dir <DIR>`",
            manager_name(status.manager),
            status.label,
            status.path.display()
        )
    } else if status.installed {
        format!(
            "{} daemon watch service {} is installed at {} but is not running; rerun `stackctl setup --dir <DIR>`",
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
    if args.delete_data {
        return handle_delete_data_uninstall();
    }
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

fn handle_delete_data_uninstall() -> Result<()> {
    let service_before = daemon::service_status()?;
    let Some(runtime_directory) =
        super::delete_data_uninstall::prepare_delete_data_uninstall(service_before.installed)?
    else {
        return Ok(());
    };
    let status = daemon::uninstall_service()?;
    super::delete_data_uninstall::remove_deleted_runtime_directory(&runtime_directory)?;
    let action = if status.installed {
        "Removed"
    } else {
        "Found no"
    };
    output::event(
        "daemon",
        LogLevel::Success,
        &format!(
            "{action} {} daemon watch service {}; deleted verified backups, state, trust material, and exact Engine resources",
            manager_name(status.manager),
            status.label,
        ),
        Persistence::Persistent,
    );

    Ok(())
}

fn validate_uninstall_mode(args: &DaemonServiceUninstallArgs) -> Result<()> {
    if args.delete_data && !args.confirm_delete_data {
        anyhow::bail!("delete-data uninstall requires --confirm-delete-data");
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
    fn confirmed_delete_data_is_an_available_uninstall_mode() {
        validate_uninstall_mode(&DaemonServiceUninstallArgs {
            keep_data: false,
            delete_data: true,
            confirm_delete_data: true,
        })
        .expect("confirmed delete-data mode");
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

    #[test]
    fn service_install_uses_complete_setup_arguments() {
        let install = DaemonServiceInstallArgs {
            dir: vec![std::path::PathBuf::from("/work")],
            interval: 17,
        };

        let setup = setup_args(&install);

        assert_eq!(setup.dir, install.dir);
        assert_eq!(setup.interval, install.interval);
    }
}
