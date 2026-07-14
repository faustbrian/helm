use super::{
    CommandExecutionId, CommandExecutor, CommandRequest, CommandSession, CommandStatus,
    ContainerId, EngineFuture, OwnedContainer, V7ContainerCommandExecutor,
    V7ContainerCommandTarget,
};

/// Internal transport adapter shared by v8 and accepted-v7 command runners.
pub(super) trait CommandSessionExecutor<Target> {
    fn start_session<'operation>(
        &'operation self,
        target: &'operation Target,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession>;

    fn session_status<'operation>(
        &'operation self,
        execution_id: &'operation CommandExecutionId,
        container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus>;
}

impl<E: CommandExecutor + ?Sized> CommandSessionExecutor<OwnedContainer> for E {
    fn start_session<'operation>(
        &'operation self,
        target: &'operation OwnedContainer,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        self.start_command(target, request)
    }

    fn session_status<'operation>(
        &'operation self,
        execution_id: &'operation CommandExecutionId,
        container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus> {
        self.command_status(execution_id, container_id)
    }
}

impl<E: V7ContainerCommandExecutor + ?Sized> CommandSessionExecutor<V7ContainerCommandTarget>
    for E
{
    fn start_session<'operation>(
        &'operation self,
        target: &'operation V7ContainerCommandTarget,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        self.start_v7_command(target, request)
    }

    fn session_status<'operation>(
        &'operation self,
        execution_id: &'operation CommandExecutionId,
        container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus> {
        self.v7_command_status(execution_id, container_id)
    }
}
