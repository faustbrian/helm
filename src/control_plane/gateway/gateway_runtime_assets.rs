use super::StoredGatewayBootstrapPaths;
use super::{CaddyEngineAdminClient, CaddyGatewayProvider, GatewayRuntimeAssetError};
use crate::control_plane::engine::{
    CommandExecutor, ContainerCreateOptions, ContainerId, ObservedContainer,
    reconstruct_owned_container,
};
use crate::control_plane::tls::LocalCertificateReconcileAction;
use std::time::Duration;

pub(crate) const CONTAINER_CERTIFICATE_PATH: &str = "/etc/stackctl/tls/wildcard.crt";
pub(crate) const CONTAINER_PRIVATE_KEY_PATH: &str = "/etc/stackctl/tls/wildcard.key";
pub(crate) const CONTAINER_ADMIN_ADDRESS: &str = "localhost:2019";

/// Verified host assets and exact Engine request for the singleton gateway.
pub(crate) struct GatewayRuntimeAssets {
    request: ContainerCreateOptions,
    bootstrap_paths: StoredGatewayBootstrapPaths,
    certificate_action: LocalCertificateReconcileAction,
    certificate_revision: String,
    certificate_was_expired: bool,
}

impl GatewayRuntimeAssets {
    pub(super) const fn new(
        request: ContainerCreateOptions,
        bootstrap_paths: StoredGatewayBootstrapPaths,
        certificate_action: LocalCertificateReconcileAction,
        certificate_revision: String,
        certificate_was_expired: bool,
    ) -> Self {
        Self {
            request,
            bootstrap_paths,
            certificate_action,
            certificate_revision,
            certificate_was_expired,
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

    pub(crate) fn certificate_revision(&self) -> &str {
        &self.certificate_revision
    }

    pub(crate) const fn certificate_was_expired(&self) -> bool {
        self.certificate_was_expired
    }

    pub(crate) fn configuration_provider<E>(
        &self,
        engine: E,
    ) -> Result<CaddyGatewayProvider<CaddyEngineAdminClient<E>>, GatewayRuntimeAssetError>
    where
        E: CommandExecutor + Send + Sync,
    {
        let observed = ObservedContainer::new(
            ContainerId::new(self.request.name()),
            self.request.metadata().labels(),
        );
        let container = reconstruct_owned_container(
            &observed,
            self.request.metadata().installation_id(),
            self.request.metadata().schema_version(),
        )
        .map_err(|error| {
            GatewayRuntimeAssetError::Gateway(super::GatewayError::InvalidPlan {
                detail: format!("gateway request ownership is invalid: {error:?}"),
            })
        })?;
        Ok(CaddyGatewayProvider::new(
            CaddyEngineAdminClient::new(engine, container, Duration::from_secs(10)),
            CONTAINER_CERTIFICATE_PATH,
            CONTAINER_PRIVATE_KEY_PATH,
            CONTAINER_ADMIN_ADDRESS,
        ))
    }
}
