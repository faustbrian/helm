//! Service networking helpers.

use super::ServiceConfig;

impl ServiceConfig {
    /// Returns whether runtime access should resolve through host-gateway alias.
    #[must_use]
    pub fn uses_host_gateway_alias(&self) -> bool {
        self.host == "127.0.0.1" || self.host.eq_ignore_ascii_case("localhost")
    }
}
