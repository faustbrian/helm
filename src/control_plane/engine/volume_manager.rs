use super::{EngineFuture, OwnedVolume, VolumeCreateOptions};

/// Narrow strategy that cannot delete a volume without ownership proof.
pub(crate) trait VolumeManager {
    fn create_volume<'operation>(
        &'operation mut self,
        options: &'operation VolumeCreateOptions,
    ) -> EngineFuture<'operation, OwnedVolume>;

    fn remove_volume<'operation>(
        &'operation mut self,
        volume: &'operation OwnedVolume,
    ) -> EngineFuture<'operation, ()>;
}
