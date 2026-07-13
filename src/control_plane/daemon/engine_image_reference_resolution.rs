use super::ImageReferenceResolution;
use crate::control_plane::engine::{
    BollardEngineAdapter, ImageReferenceResolver, RegistryImageReference,
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
        self.runtime.block_on(async {
            let mut resolved = BTreeMap::new();
            for (id, source) in references {
                let reference = RegistryImageReference::new(source).map_err(|error| {
                    format!("image reference '{id}' cannot be resolved: {error}")
                })?;
                let immutable = self
                    .engine
                    .resolve_image_reference(&reference)
                    .await
                    .map_err(|error| {
                        format!("image reference '{id}' cannot be resolved: {error}")
                    })?;
                resolved.insert(id.clone(), immutable.as_str().to_owned());
            }

            Ok(resolved)
        })
    }
}
