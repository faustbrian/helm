use super::ImageReferenceResolution;
use crate::control_plane::engine::{
    BollardEngineAdapter, resolve_registry_image_references_with_engine,
};
use std::collections::BTreeMap;

/// Bridges synchronous IPC dispatch to the selected async Engine adapter.
pub(crate) struct EngineImageReferenceResolution<'runtime> {
    runtime: &'runtime tokio::runtime::Runtime,
    engine: BollardEngineAdapter,
}

impl<'runtime> EngineImageReferenceResolution<'runtime> {
    pub(crate) const fn new(
        runtime: &'runtime tokio::runtime::Runtime,
        engine: BollardEngineAdapter,
    ) -> Self {
        Self { runtime, engine }
    }
}

impl ImageReferenceResolution for EngineImageReferenceResolution<'_> {
    fn resolve(
        &mut self,
        references: &BTreeMap<String, String>,
    ) -> Result<BTreeMap<String, String>, String> {
        self.runtime
            .block_on(resolve_registry_image_references_with_engine(
                &mut self.engine,
                references,
            ))
            .map_err(|error| error.to_string())
    }
}
