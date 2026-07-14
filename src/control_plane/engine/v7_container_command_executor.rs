use super::{
    CommandExecutionId, CommandRequest, CommandSession, CommandStatus, ContainerId, EngineFuture,
    V7ContainerCommandTarget,
};

/// Narrow command capability for an immutable, accepted v7 container identity.
pub(crate) trait V7ContainerCommandExecutor {
    fn start_v7_command<'operation>(
        &'operation self,
        target: &'operation V7ContainerCommandTarget,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession>;

    fn v7_command_status<'operation>(
        &'operation self,
        execution_id: &'operation CommandExecutionId,
        container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus>;
}
