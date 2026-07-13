/// Complete native Caddy JSON configuration and its desired revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CaddyGatewayDocument {
    revision: String,
    bytes: Vec<u8>,
}

impl CaddyGatewayDocument {
    pub(super) const fn new(revision: String, bytes: Vec<u8>) -> Self {
        Self { revision, bytes }
    }

    pub(crate) fn revision(&self) -> &str {
        &self.revision
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
