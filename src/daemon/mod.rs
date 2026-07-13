//! Login-service integration for the per-user singleton daemon.

mod service;

pub(crate) use service::{
    DaemonServiceInstallOptions, ServiceManager, install_service, print_service, service_status,
    uninstall_service,
};
