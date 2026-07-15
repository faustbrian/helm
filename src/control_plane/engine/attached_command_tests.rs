use super::{
    AttachedCommandOptions, CommandExecutionId, CommandExecutor, CommandRequest, CommandSession,
    CommandStatus, ContainerId, ContainerLogStream, EngineError, EngineFuture,
    ManagedResourceMetadata, ManagedResourceMetadataOptions, OwnedContainer, ResourceKind,
    RetentionClass, run_attached_command,
};
use futures_util::stream;
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::io::duplex;

#[test]
fn attached_command_preserves_nonzero_container_exit_status() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("attached-command runtime");
    let request = CommandRequest::new(
        vec!["psql".to_owned()],
        BTreeMap::from([("PGPASSWORD".to_owned(), "hidden".to_owned())]),
        None,
    )
    .expect("attached-command request");
    let options = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "provision PostgreSQL logical resource",
        Duration::from_secs(5),
    )
    .expect("attached-command options");

    let error = runtime
        .block_on(run_attached_command(
            &ExitedCommandExecutor,
            &owned_container(),
            &options,
        ))
        .expect_err("nonzero command status");

    assert_eq!(
        error,
        EngineError::ContainerExit {
            container_id: "postgres-17".to_owned(),
            status_code: 9,
        }
    );
    assert!(!error.to_string().contains("hidden"));
}

struct ExitedCommandExecutor;

impl CommandExecutor for ExitedCommandExecutor {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        _request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        let container_id = container.id().clone();

        Box::pin(async move {
            let (writer, _reader) = duplex(64);
            let output: ContainerLogStream<'static> = Box::pin(stream::empty());

            Ok(CommandSession::new(
                CommandExecutionId::new("attached-exec"),
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
        Box::pin(async { Ok(CommandStatus::Exited(9)) })
    }
}

fn owned_container() -> OwnedContainer {
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::SharedService,
        project_id: None,
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        schema_version: 8,
        desired_revision: "sha256:postgres-17-v1".to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("shared-service metadata");

    OwnedContainer::new(ContainerId::new("postgres-17"), metadata)
}
