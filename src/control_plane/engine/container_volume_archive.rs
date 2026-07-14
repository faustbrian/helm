use super::{EngineFuture, OwnedContainer, OwnedVolume};
use std::path::Path;
use tokio::io::AsyncWrite;

/// Streams one exact owned named volume through its owning container mount.
pub(crate) trait ContainerVolumeArchive {
    fn download_volume_archive<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        volume: &'operation OwnedVolume,
        output: &'operation mut (dyn AsyncWrite + Send + Unpin),
    ) -> EngineFuture<'operation, ()>;

    fn upload_volume_archive<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        volume: &'operation OwnedVolume,
        archive: &'operation Path,
    ) -> EngineFuture<'operation, ()>;

    fn download_volume_subpath_archive<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        volume: &'operation OwnedVolume,
        relative_path: &'operation Path,
        output: &'operation mut (dyn AsyncWrite + Send + Unpin),
    ) -> EngineFuture<'operation, ()>;

    fn upload_volume_subpath_archive<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        volume: &'operation OwnedVolume,
        relative_path: &'operation Path,
        archive: &'operation Path,
    ) -> EngineFuture<'operation, ()>;
}
