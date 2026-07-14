use super::{
    AttachedCommandOptions, AttachedCommandOutput, CommandExecutor, CommandSessionExecutor,
    CommandStatus, EngineError, OwnedContainer, V7ContainerCommandExecutor,
    V7ContainerCommandTarget,
};
use futures_util::StreamExt;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

const STATUS_POLL_MILLISECONDS: u64 = 20;
const MAX_CAPTURED_OUTPUT_BYTES: usize = 1024 * 1024;

/// Streams secret-safe input and requires a successful bounded exec status.
pub(crate) async fn run_attached_command(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    options: &AttachedCommandOptions,
) -> Result<(), EngineError> {
    run_attached_command_output(executor, container, options)
        .await
        .map(|_| ())
}

/// Streams secret-safe input and returns bounded successful standard output.
pub(crate) async fn run_attached_command_capture(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    options: &AttachedCommandOptions,
) -> Result<Vec<u8>, EngineError> {
    run_attached_command_output(executor, container, options)
        .await
        .map(AttachedCommandOutput::into_stdout)
}

/// Returns bounded stdout and stderr without lossy decoding or host processes.
pub(crate) async fn run_attached_command_output(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    options: &AttachedCommandOptions,
) -> Result<AttachedCommandOutput, EngineError> {
    run_attached_command_output_for(executor, container, options).await
}

/// Runs one bounded secret-safe command against an accepted v7 target.
pub(crate) async fn run_v7_attached_command(
    executor: &(impl V7ContainerCommandExecutor + ?Sized),
    target: &V7ContainerCommandTarget,
    options: &AttachedCommandOptions,
) -> Result<(), EngineError> {
    run_v7_attached_command_capture(executor, target, options)
        .await
        .map(|_| ())
}

/// Returns bounded stdout for a successful accepted-v7 command.
pub(crate) async fn run_v7_attached_command_capture(
    executor: &(impl V7ContainerCommandExecutor + ?Sized),
    target: &V7ContainerCommandTarget,
    options: &AttachedCommandOptions,
) -> Result<Vec<u8>, EngineError> {
    run_attached_command_output_for(executor, target, options)
        .await
        .map(AttachedCommandOutput::into_stdout)
}

async fn run_attached_command_output_for<E, Target>(
    executor: &E,
    target: &Target,
    options: &AttachedCommandOptions,
) -> Result<AttachedCommandOutput, EngineError>
where
    E: CommandSessionExecutor<Target> + ?Sized,
{
    tokio::time::timeout(options.timeout(), execute(executor, target, options))
        .await
        .map_err(|_| EngineError::Timeout {
            action: options.action().to_owned(),
            timeout_milliseconds: u64::try_from(options.timeout().as_millis()).unwrap_or(u64::MAX),
        })?
}

async fn execute<E, Target>(
    executor: &E,
    target: &Target,
    options: &AttachedCommandOptions,
) -> Result<AttachedCommandOutput, EngineError>
where
    E: CommandSessionExecutor<Target> + ?Sized,
{
    let session = executor.start_session(target, options.request()).await?;
    let (execution_id, container_id, mut input, mut output) = session.into_parts();

    input
        .write_all(options.input())
        .await
        .map_err(|error| EngineError::Backend {
            detail: format!("failed to write {} input: {error}", options.action()),
        })?;
    input
        .shutdown()
        .await
        .map_err(|error| EngineError::Backend {
            detail: format!("failed to close {} input: {error}", options.action()),
        })?;
    drop(input);

    let mut captured = AttachedCommandOutput::new();
    while let Some(chunk) = output.next().await {
        let chunk = chunk?;
        let output_bytes = captured.len().saturating_add(chunk.bytes().len());
        if output_bytes > MAX_CAPTURED_OUTPUT_BYTES {
            return Err(EngineError::Backend {
                detail: format!(
                    "{} output exceeds {} bytes",
                    options.action(),
                    MAX_CAPTURED_OUTPUT_BYTES
                ),
            });
        }
        if chunk.is_stderr() {
            captured.extend_stderr(chunk.bytes());
        } else {
            captured.extend_stdout(chunk.bytes());
        }
    }

    loop {
        match executor
            .session_status(&execution_id, &container_id)
            .await?
        {
            CommandStatus::Running => {
                tokio::time::sleep(Duration::from_millis(STATUS_POLL_MILLISECONDS)).await;
            }
            CommandStatus::Exited(0) => return Ok(captured),
            CommandStatus::Exited(status) => {
                return Err(EngineError::Backend {
                    detail: format!("{} exited with status {status}", options.action()),
                });
            }
        }
    }
}
