use super::{EngineFuture, OwnedImage};

/// Narrow strategy that cannot delete an image without ownership proof.
pub(crate) trait ImageManager {
    fn remove_image<'operation>(
        &'operation mut self,
        image: &'operation OwnedImage,
    ) -> EngineFuture<'operation, ()>;
}
