use super::{BollardEngineAdapter, EngineError, ImageReferenceResolver, RegistryImageReference};
use std::collections::BTreeMap;
use std::path::Path;

/// Resolves mutable registry references through a directly selected Engine.
#[cfg(unix)]
pub(crate) fn resolve_registry_image_references(
    socket_path: &Path,
    references: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, EngineError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| EngineError::Backend {
            detail: format!("failed to initialize image-resolution runtime: {error}"),
        })?;
    let mut engine = runtime.block_on(BollardEngineAdapter::connect_unix(socket_path))?;

    runtime.block_on(resolve_registry_image_references_with_engine(
        &mut engine,
        references,
    ))
}

pub(crate) async fn resolve_registry_image_references_with_engine<Engine>(
    engine: &mut Engine,
    references: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, EngineError>
where
    Engine: ImageReferenceResolver,
{
    let mut resolved = BTreeMap::new();
    for (id, source) in references {
        let reference =
            RegistryImageReference::new(source).map_err(|error| EngineError::InvalidRequest {
                detail: format!("image reference '{id}' cannot be resolved: {error}"),
            })?;
        let immutable = engine
            .resolve_image_reference(&reference)
            .await
            .map_err(|error| EngineError::Backend {
                detail: format!("image reference '{id}' cannot be resolved: {error}"),
            })?;
        resolved.insert(id.clone(), immutable.as_str().to_owned());
    }

    Ok(resolved)
}
