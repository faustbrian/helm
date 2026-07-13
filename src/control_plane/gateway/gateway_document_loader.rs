use super::GatewayFuture;

/// Transport-only capability for atomically loading a complete gateway document.
pub(crate) trait GatewayDocumentLoader {
    fn load_document<'operation>(
        &'operation mut self,
        document: &'operation [u8],
    ) -> GatewayFuture<'operation, ()>;
}
