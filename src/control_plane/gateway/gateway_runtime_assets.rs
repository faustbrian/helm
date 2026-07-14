use super::StoredGatewayBootstrapPaths;
#[cfg(unix)]
use super::{CaddyGatewayProvider, CaddyUnixAdminClient};
use crate::control_plane::engine::ContainerCreateOptions;
use crate::control_plane::tls::{LocalCertificateReconcileAction, StoredCertificatePaths};

pub(crate) const CONTAINER_CERTIFICATE_PATH: &str = "/etc/stackctl/tls/wildcard.crt";
pub(crate) const CONTAINER_PRIVATE_KEY_PATH: &str = "/etc/stackctl/tls/wildcard.key";
pub(crate) const CONTAINER_ADMIN_SOCKET_PATH: &str = "/run/stackctl/admin.sock";

/// Verified host assets and exact Engine request for the singleton gateway.
pub(crate) struct GatewayRuntimeAssets {
    request: ContainerCreateOptions,
    bootstrap_paths: StoredGatewayBootstrapPaths,
    certificate_paths: StoredCertificatePaths,
    certificate_action: LocalCertificateReconcileAction,
}

impl GatewayRuntimeAssets {
    pub(super) const fn new(
        request: ContainerCreateOptions,
        bootstrap_paths: StoredGatewayBootstrapPaths,
        certificate_paths: StoredCertificatePaths,
        certificate_action: LocalCertificateReconcileAction,
    ) -> Self {
        Self {
            request,
            bootstrap_paths,
            certificate_paths,
            certificate_action,
        }
    }

    pub(crate) const fn request(&self) -> &ContainerCreateOptions {
        &self.request
    }

    pub(crate) const fn bootstrap_paths(&self) -> &StoredGatewayBootstrapPaths {
        &self.bootstrap_paths
    }

    pub(crate) const fn certificate_paths(&self) -> &StoredCertificatePaths {
        &self.certificate_paths
    }

    pub(crate) const fn certificate_action(&self) -> LocalCertificateReconcileAction {
        self.certificate_action
    }

    #[cfg(unix)]
    pub(crate) fn configuration_provider(&self) -> CaddyGatewayProvider<CaddyUnixAdminClient> {
        CaddyGatewayProvider::new(
            CaddyUnixAdminClient::new(self.bootstrap_paths.admin_socket_path()),
            CONTAINER_CERTIFICATE_PATH,
            CONTAINER_PRIVATE_KEY_PATH,
            CONTAINER_ADMIN_SOCKET_PATH,
        )
    }
}
