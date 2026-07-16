use super::{
    GatewayConfiguration, GatewayDocumentLoader, GatewayFuture, GatewaySnapshot,
    render_caddy_document,
};
use std::path::PathBuf;

/// Atomic Caddy provider independent of how the private admin API is transported.
pub(crate) struct CaddyGatewayProvider<Loader> {
    loader: Loader,
    certificate_path: PathBuf,
    private_key_path: PathBuf,
    admin_address: String,
    active_revision: Option<String>,
}

impl<Loader> CaddyGatewayProvider<Loader> {
    pub(crate) fn new(
        loader: Loader,
        certificate_path: impl Into<PathBuf>,
        private_key_path: impl Into<PathBuf>,
        admin_address: impl Into<String>,
    ) -> Self {
        Self {
            loader,
            certificate_path: certificate_path.into(),
            private_key_path: private_key_path.into(),
            admin_address: admin_address.into(),
            active_revision: None,
        }
    }

    #[cfg(test)]
    pub(crate) const fn loader(&self) -> &Loader {
        &self.loader
    }
}

impl<Loader> GatewayConfiguration for CaddyGatewayProvider<Loader>
where
    Loader: GatewayDocumentLoader + Send,
{
    fn apply_snapshot<'operation>(
        &'operation mut self,
        snapshot: &'operation GatewaySnapshot,
    ) -> GatewayFuture<'operation, ()> {
        Box::pin(async move {
            let document = render_caddy_document(
                snapshot,
                &self.certificate_path,
                &self.private_key_path,
                &self.admin_address,
            )?;
            self.loader.load_document(document.bytes()).await?;
            self.active_revision = Some(document.revision().to_owned());

            Ok(())
        })
    }

    fn active_revision(&self) -> GatewayFuture<'_, Option<String>> {
        let active_revision = self.active_revision.clone();

        Box::pin(async move { Ok(active_revision) })
    }
}
