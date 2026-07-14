use super::{
    CommandExecutor, CommandSessionExecutor, CommandStatus, EngineError, OwnedContainer,
    StreamingCommandOptions, V7ContainerCommandExecutor, V7ContainerCommandTarget,
};
use futures_util::StreamExt;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

const STATUS_POLL_MILLISECONDS: u64 = 20;

/// Streams attached exec stdin and stdout concurrently with bounded execution.
pub(crate) async fn run_streaming_command<R, W>(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    options: &StreamingCommandOptions,
    input: &mut R,
    output: &mut W,
) -> Result<(), EngineError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    run_streaming_command_for(executor, container, options, input, output).await
}

/// Runs the shared bounded transport against an exact accepted v7 target.
pub(crate) async fn run_v7_streaming_command<R, W>(
    executor: &(impl V7ContainerCommandExecutor + ?Sized),
    target: &V7ContainerCommandTarget,
    options: &StreamingCommandOptions,
    input: &mut R,
    output: &mut W,
) -> Result<(), EngineError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    run_streaming_command_for(executor, target, options, input, output).await
}

async fn run_streaming_command_for<R, W, E, Target>(
    executor: &E,
    target: &Target,
    options: &StreamingCommandOptions,
    input: &mut R,
    output: &mut W,
) -> Result<(), EngineError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    E: CommandSessionExecutor<Target> + ?Sized,
{
    tokio::time::timeout(
        options.timeout(),
        execute(executor, target, options, input, output),
    )
    .await
    .map_err(|_| EngineError::Timeout {
        action: options.action().to_owned(),
        timeout_milliseconds: u64::try_from(options.timeout().as_millis()).unwrap_or(u64::MAX),
    })?
}

async fn execute<R, W, E, Target>(
    executor: &E,
    target: &Target,
    options: &StreamingCommandOptions,
    input: &mut R,
    output: &mut W,
) -> Result<(), EngineError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    E: CommandSessionExecutor<Target> + ?Sized,
{
    let session = executor.start_session(target, options.request()).await?;
    let (execution_id, container_id, mut command_input, mut command_output) = session.into_parts();
    let write_input = async {
        tokio::io::copy(input, &mut command_input)
            .await
            .map_err(|error| backend_error(options, "stream command input", error))?;
        command_input
            .shutdown()
            .await
            .map_err(|error| backend_error(options, "close command input", error))
    };
    let read_output = async {
        while let Some(chunk) = command_output.next().await {
            let chunk = chunk?;
            if !chunk.is_stderr() {
                output
                    .write_all(chunk.bytes())
                    .await
                    .map_err(|error| backend_error(options, "stream command output", error))?;
            }
        }
        output
            .flush()
            .await
            .map_err(|error| backend_error(options, "flush command output", error))
    };

    futures_util::future::try_join(write_input, read_output).await?;

    loop {
        match executor
            .session_status(&execution_id, &container_id)
            .await?
        {
            CommandStatus::Running => {
                tokio::time::sleep(Duration::from_millis(STATUS_POLL_MILLISECONDS)).await;
            }
            CommandStatus::Exited(0) => return Ok(()),
            CommandStatus::Exited(status) => {
                return Err(EngineError::Backend {
                    detail: format!("{} exited with status {status}", options.action()),
                });
            }
        }
    }
}

fn backend_error(
    options: &StreamingCommandOptions,
    operation: &str,
    error: std::io::Error,
) -> EngineError {
    EngineError::Backend {
        detail: format!("failed to {operation} for {}: {error}", options.action()),
    }
}
