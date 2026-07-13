use super::StoredGatewayBootstrapPaths;
use crate::control_plane::engine::ContainerCreateOptions;
use crate::control_plane::tls::LocalCertificateReconcileAction;

/// Verified host assets and exact Engine request for the singleton gateway.
pub(crate) struct GatewayRuntimeAssets {
    request: ContainerCreateOptions,
    bootstrap_paths: StoredGatewayBootstrapPaths,
    certificate_action: LocalCertificateReconcileAction,
}

impl GatewayRuntimeAssets {
    pub(super) const fn new(
        request: ContainerCreateOptions,
        bootstrap_paths: StoredGatewayBootstrapPaths,
        certificate_action: LocalCertificateReconcileAction,
    ) -> Self {
        Self {
            request,
            bootstrap_paths,
            certificate_action,
        }
    }

    pub(crate) const fn request(&self) -> &ContainerCreateOptions {
        &self.request
    }

    pub(crate) const fn bootstrap_paths(&self) -> &StoredGatewayBootstrapPaths {
        &self.bootstrap_paths
    }

    pub(crate) const fn certificate_action(&self) -> LocalCertificateReconcileAction {
        self.certificate_action
    }
}
