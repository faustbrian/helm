use super::{CommandExecutionId, ContainerId, ContainerLogStream};
use std::pin::Pin;
use tokio::io::AsyncWrite;

/// Writable stdin channel for one attached Engine exec process.
pub(crate) type CommandInput = Pin<Box<dyn AsyncWrite + Send>>;

/// Attached structured Engine exec I/O and its opaque identity.
pub(crate) struct CommandSession {
    execution_id: CommandExecutionId,
    container_id: ContainerId,
    input: CommandInput,
    output: ContainerLogStream<'static>,
}

impl CommandSession {
    pub(crate) const fn new(
        execution_id: CommandExecutionId,
        container_id: ContainerId,
        input: CommandInput,
        output: ContainerLogStream<'static>,
    ) -> Self {
        Self {
            execution_id,
            container_id,
            input,
            output,
        }
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        CommandExecutionId,
        ContainerId,
        CommandInput,
        ContainerLogStream<'static>,
    ) {
        (
            self.execution_id,
            self.container_id,
            self.input,
            self.output,
        )
    }
}
