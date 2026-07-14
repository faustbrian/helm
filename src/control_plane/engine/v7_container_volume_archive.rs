use super::{ContainerState, EngineFuture, V7ContainerCommandTarget, VolumeMount};
use tokio::io::AsyncWrite;

/// Recovery-only access to exact accepted-v7 named-volume mounts.
pub(crate) trait V7ContainerVolumeArchive {
    fn inspect_v7_volume_container<'operation>(
        &'operation self,
        target: &'operation V7ContainerCommandTarget,
        mounts: &'operation [VolumeMount],
    ) -> EngineFuture<'operation, ContainerState>;

    fn start_v7_volume_container<'operation>(
        &'operation self,
        target: &'operation V7ContainerCommandTarget,
        mounts: &'operation [VolumeMount],
    ) -> EngineFuture<'operation, ()>;

    fn stop_v7_volume_container<'operation>(
        &'operation self,
        target: &'operation V7ContainerCommandTarget,
        mounts: &'operation [VolumeMount],
    ) -> EngineFuture<'operation, ()>;

    fn download_v7_volume_archive<'operation>(
        &'operation self,
        target: &'operation V7ContainerCommandTarget,
        mounts: &'operation [VolumeMount],
        volume_name: &'operation str,
        output: &'operation mut (dyn AsyncWrite + Send + Unpin),
    ) -> EngineFuture<'operation, ()>;
}
