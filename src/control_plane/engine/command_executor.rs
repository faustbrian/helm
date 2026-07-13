use super::{
    CommandExecutionId, CommandRequest, CommandSession, CommandStatus, ContainerId, EngineFuture,
    OwnedContainer,
};

/// Narrow Engine capability for attached non-shell container commands.
pub(crate) trait CommandExecutor {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession>;

    fn command_status<'operation>(
        &'operation self,
        execution_id: &'operation CommandExecutionId,
        container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus>;
}
