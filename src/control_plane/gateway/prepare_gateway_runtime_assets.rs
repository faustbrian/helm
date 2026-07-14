use super::{
    CONTAINER_ADMIN_SOCKET_PATH, CONTAINER_CERTIFICATE_PATH, CONTAINER_PRIVATE_KEY_PATH,
    GatewayRuntimeAssetError, GatewayRuntimeAssetOptions, GatewayRuntimeAssets, GatewaySnapshot,
    GlobalGatewayRequestOptions, global_gateway_request, render_caddy_document,
    store_caddy_bootstrap,
};
use crate::control_plane::tls::{FilesystemCertificateStore, reconcile_local_certificates};
use std::path::Path;

/// Recovers or creates all private host assets before gateway reconciliation.
pub(crate) fn prepare_gateway_runtime_assets(
    options: GatewayRuntimeAssetOptions<'_>,
) -> Result<GatewayRuntimeAssets, GatewayRuntimeAssetError> {
    let certificate_store = FilesystemCertificateStore::new(options.runtime_directory.join("tls"));
    let current = certificate_store.load_current()?;
    let reconciliation =
        reconcile_local_certificates(current.as_ref().map(|(bundle, _paths)| bundle), options.now)?;
    let certificate_paths = certificate_store.persist(reconciliation.bundle())?;
    let certificate_revision = certificate_revision(certificate_paths.directory())?;

    let gateway_directory = options.runtime_directory.join("gateway");
    let config_path = gateway_directory.join("bootstrap.json");
    let admin_runtime_directory = gateway_directory.join("run");
    let snapshot = GatewaySnapshot::new(Vec::new())?;
    let document = render_caddy_document(
        &snapshot,
        Path::new(CONTAINER_CERTIFICATE_PATH),
        Path::new(CONTAINER_PRIVATE_KEY_PATH),
        Path::new(CONTAINER_ADMIN_SOCKET_PATH),
    )?;
    let bootstrap_paths = store_caddy_bootstrap(&document, &config_path, &admin_runtime_directory)?;
    let request = global_gateway_request(GlobalGatewayRequestOptions {
        installation_id: options.installation_id.to_owned(),
        container_user: options.container_user.to_owned(),
        certificate_path: certificate_paths.leaf_certificate(),
        private_key_path: certificate_paths.leaf_private_key(),
        certificate_revision,
        bootstrap_config_path: bootstrap_paths.config_path().to_path_buf(),
        admin_runtime_directory: bootstrap_paths.runtime_directory().to_path_buf(),
    })?;

    Ok(GatewayRuntimeAssets::new(
        request,
        bootstrap_paths,
        certificate_paths,
        reconciliation.action(),
    ))
}

fn certificate_revision(directory: &Path) -> Result<String, GatewayRuntimeAssetError> {
    let revision = directory
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("bundle-"))
        .filter(|revision| !revision.is_empty())
        .ok_or_else(|| {
            GatewayRuntimeAssetError::Gateway(super::GatewayError::InvalidPlan {
                detail: format!(
                    "stored certificate directory '{}' has no immutable bundle revision",
                    directory.display()
                ),
            })
        })?;

    Ok(revision.to_owned())
}
