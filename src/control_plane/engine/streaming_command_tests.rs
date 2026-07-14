use super::{
    CommandExecutionId, CommandExecutor, CommandRequest, CommandSession, CommandStatus,
    ContainerId, ContainerLogStream, EngineFuture, LogChunk, LogStreamKind,
    ManagedResourceMetadata, ManagedResourceMetadataOptions, OwnedContainer, ResourceKind,
    RetentionClass, StreamingCommandOptions, run_streaming_command,
};
use futures_util::stream;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, duplex};

#[test]
fn streaming_command_moves_large_bidirectional_payloads_without_buffer_limits() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("streaming runtime");
    let input = vec![b'i'; 2 * 1024 * 1024 + 17];
    let stdout = vec![b'o'; 2 * 1024 * 1024 + 31];
    let executor = StreamingExecutor::new(stdout.clone(), 0);
    let request = CommandRequest::new(
        vec!["pg_restore".to_owned(), "--dbname=staging".to_owned()],
        BTreeMap::from([("PGPASSWORD".to_owned(), "hidden".to_owned())]),
        None,
    )
    .expect("streaming request");
    let options =
        StreamingCommandOptions::new(request, "restore PostgreSQL backup", Duration::from_secs(5))
            .expect("streaming options");
    let mut input_reader = std::io::Cursor::new(input.clone());
    let mut captured = Vec::new();

    runtime
        .block_on(run_streaming_command(
            &executor,
            &owned_container(),
            &options,
            &mut input_reader,
            &mut captured,
        ))
        .expect("stream command");
    runtime.block_on(tokio::task::yield_now());

    assert_eq!(captured, stdout);
    assert_eq!(*executor.input.lock().expect("captured input"), input);
    assert!(!format!("{options:?}").contains("hidden"));
}

#[test]
fn streaming_command_drains_stderr_without_exposing_its_contents() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("streaming runtime");
    let executor = StreamingExecutor::with_stderr(b"password=do-not-log".to_vec(), 9);
    let request = CommandRequest::new(vec!["pg_dump".to_owned()], BTreeMap::new(), None)
        .expect("streaming request");
    let options =
        StreamingCommandOptions::new(request, "dump PostgreSQL database", Duration::from_secs(5))
            .expect("streaming options");
    let mut input = std::io::Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();

    let error = runtime
        .block_on(run_streaming_command(
            &executor,
            &owned_container(),
            &options,
            &mut input,
            &mut output,
        ))
        .expect_err("failed command");

    assert_eq!(
        error.to_string(),
        "dump PostgreSQL database exited with status 9"
    );
    assert!(!error.to_string().contains("do-not-log"));
    assert!(output.is_empty());
}

struct StreamingExecutor {
    output: Vec<LogChunk>,
    input: Arc<Mutex<Vec<u8>>>,
    exit_status: i64,
}

impl StreamingExecutor {
    fn new(stdout: Vec<u8>, exit_status: i64) -> Self {
        Self {
            output: stdout
                .chunks(8 * 1024)
                .map(|chunk| LogChunk::stdout(chunk.to_vec()))
                .collect(),
            input: Arc::new(Mutex::new(Vec::new())),
            exit_status,
        }
    }

    fn with_stderr(stderr: Vec<u8>, exit_status: i64) -> Self {
        Self {
            output: vec![LogChunk::new(LogStreamKind::Stderr, stderr)],
            input: Arc::new(Mutex::new(Vec::new())),
            exit_status,
        }
    }
}

impl CommandExecutor for StreamingExecutor {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        _request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        let captured = Arc::clone(&self.input);
        let output = self.output.clone();
        let container_id = container.id().clone();

        Box::pin(async move {
            let (writer, mut reader) = duplex(1_024);
            tokio::spawn(async move {
                let mut input = Vec::new();
                reader
                    .read_to_end(&mut input)
                    .await
                    .expect("read streaming input");
                *captured.lock().expect("streaming input lock") = input;
            });
            let output: ContainerLogStream<'static> =
                Box::pin(stream::iter(output.into_iter().map(Ok).collect::<Vec<_>>()));

            Ok(CommandSession::new(
                CommandExecutionId::new("streaming-exec"),
                container_id,
                Box::pin(writer),
                output,
            ))
        })
    }

    fn command_status<'operation>(
        &'operation self,
        _execution_id: &'operation CommandExecutionId,
        _container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus> {
        Box::pin(async move { Ok(CommandStatus::Exited(self.exit_status)) })
    }
}

fn owned_container() -> OwnedContainer {
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::ProvisioningJob,
        project_id: Some("bill".to_owned()),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        schema_version: 8,
        desired_revision: "sha256:migration".to_owned(),
        retention: RetentionClass::Disposable,
    })
    .expect("streaming metadata");

    OwnedContainer::new(ContainerId::new("postgres-source"), metadata)
}
