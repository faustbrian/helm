//! Login-service integration for the per-user singleton daemon.

mod service;

pub(crate) use service::{
    DaemonServiceInstallOptions, ServiceManager, canonical_watch_dirs, install_service,
    print_service, service_status, uninstall_service,
};
