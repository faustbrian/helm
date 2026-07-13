use super::{EngineFuture, ImmutableImageReference, RegistryImageReference};

/// Narrow direct-Engine capability that resolves a registry manifest identity.
pub(crate) trait ImageReferenceResolver {
    fn resolve_image_reference<'operation>(
        &'operation mut self,
        reference: &'operation RegistryImageReference,
    ) -> EngineFuture<'operation, ImmutableImageReference>;
}
