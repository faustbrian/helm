use super::EngineProvider;

/// Immutable per-user installation identity and chosen Engine endpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InstallationRecord {
    installation_id: String,
    engine_provider: EngineProvider,
    engine_endpoint: String,
}

impl InstallationRecord {
    pub(crate) fn new(
        installation_id: impl Into<String>,
        engine_provider: EngineProvider,
        engine_endpoint: impl Into<String>,
    ) -> Self {
        Self {
            installation_id: installation_id.into(),
            engine_provider,
            engine_endpoint: engine_endpoint.into(),
        }
    }

    pub(crate) fn installation_id(&self) -> &str {
        &self.installation_id
    }

    pub(crate) const fn engine_provider(&self) -> EngineProvider {
        self.engine_provider
    }

    pub(crate) fn engine_endpoint(&self) -> &str {
        &self.engine_endpoint
    }
}
