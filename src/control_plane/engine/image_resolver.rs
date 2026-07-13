use super::{EngineFuture, ImageId, ImmutableImageReference};

/// Narrow direct-Engine capability that makes an immutable image available.
pub(crate) trait ImageResolver {
    fn ensure_image<'operation>(
        &'operation mut self,
        reference: &'operation ImmutableImageReference,
    ) -> EngineFuture<'operation, ImageId>;
}
