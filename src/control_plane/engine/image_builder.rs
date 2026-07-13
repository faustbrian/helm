use super::{EngineFuture, ImageBuildRequest, ImageId};

/// Narrow Engine capability for offline content-addressed derived images.
pub(crate) trait ImageBuilder {
    fn build_image<'operation>(
        &'operation self,
        request: &'operation ImageBuildRequest,
    ) -> EngineFuture<'operation, ImageId>;
}
