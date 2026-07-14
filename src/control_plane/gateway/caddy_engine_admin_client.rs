use super::{GatewayDocumentLoader, GatewayError, GatewayFuture};
use crate::control_plane::engine::{
    CommandExecutor, CommandRequest, OwnedContainer, StreamingCommandOptions, run_streaming_command,
};
use std::collections::BTreeMap;
use std::io::Cursor;
use std::time::Duration;

const ADMIN_ADDRESS: &str = "localhost:2019";

/// Loads complete Caddy documents through an ownership-checked Engine exec.
pub(crate) struct CaddyEngineAdminClient<E> {
    engine: E,
    container: OwnedContainer,
    timeout: Duration,
}

impl<E> CaddyEngineAdminClient<E> {
    pub(crate) const fn new(engine: E, container: OwnedContainer, timeout: Duration) -> Self {
        Self {
            engine,
            container,
            timeout,
        }
    }
}

impl<E> GatewayDocumentLoader for CaddyEngineAdminClient<E>
where
    E: CommandExecutor + Send + Sync,
{
    fn load_document<'operation>(
        &'operation mut self,
        document: &'operation [u8],
    ) -> GatewayFuture<'operation, ()> {
        Box::pin(async move {
            let request = CommandRequest::new(
                vec![
                    "caddy".to_owned(),
                    "reload".to_owned(),
                    "--config".to_owned(),
                    "-".to_owned(),
                    "--address".to_owned(),
                    ADMIN_ADDRESS.to_owned(),
                ],
                BTreeMap::new(),
                None,
            )
            .map_err(provider_error)?;
            let options = StreamingCommandOptions::new(
                request,
                "load complete gateway configuration",
                self.timeout,
            )
            .map_err(provider_error)?;
            let mut input = Cursor::new(document);
            let mut output = tokio::io::sink();
            run_streaming_command(
                &self.engine,
                &self.container,
                &options,
                &mut input,
                &mut output,
            )
            .await
            .map_err(provider_error)
        })
    }
}

fn provider_error(error: impl std::fmt::Display) -> GatewayError {
    GatewayError::Provider {
        detail: format!("Caddy Engine admin reload failed: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_plane::engine::{
        CommandExecutionId, CommandSession, CommandStatus, ContainerId, ContainerLogStream,
        EngineFuture, LogChunk, ManagedResourceMetadata, ManagedResourceMetadataOptions,
        ObservedContainer, ResourceKind, RetentionClass, reconstruct_owned_container,
    };
    use futures_util::stream;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, duplex};

    #[test]
    fn loads_complete_document_through_private_engine_exec() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("test runtime");
        let executor = RecordingExecutor::default();
        let mut client =
            CaddyEngineAdminClient::new(executor.clone(), owned_gateway(), Duration::from_secs(5));

        runtime
            .block_on(client.load_document(br#"{"apps":{}}"#))
            .expect("load complete document");
        runtime.block_on(tokio::task::yield_now());

        assert_eq!(
            *executor.arguments.lock().expect("arguments lock"),
            [
                "caddy",
                "reload",
                "--config",
                "-",
                "--address",
                "localhost:2019",
            ]
        );
        assert_eq!(
            *executor.input.lock().expect("input lock"),
            br#"{"apps":{}}"#
        );
    }

    #[derive(Clone, Default)]
    struct RecordingExecutor {
        arguments: Arc<Mutex<Vec<String>>>,
        input: Arc<Mutex<Vec<u8>>>,
    }

    impl CommandExecutor for RecordingExecutor {
        fn start_command<'operation>(
            &'operation self,
            container: &'operation OwnedContainer,
            request: &'operation CommandRequest,
        ) -> EngineFuture<'operation, CommandSession> {
            *self.arguments.lock().expect("arguments lock") = request.arguments().to_vec();
            let captured = Arc::clone(&self.input);
            let container_id = container.id().clone();

            Box::pin(async move {
                let (writer, mut reader) = duplex(1_024);
                tokio::spawn(async move {
                    let mut input = Vec::new();
                    reader.read_to_end(&mut input).await.expect("read input");
                    *captured.lock().expect("input lock") = input;
                });
                let output: ContainerLogStream<'static> =
                    Box::pin(stream::iter(Vec::<Result<LogChunk, _>>::new()));

                Ok(CommandSession::new(
                    CommandExecutionId::new("gateway-reload"),
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
            Box::pin(async { Ok(CommandStatus::Exited(0)) })
        }
    }

    fn owned_gateway() -> OwnedContainer {
        let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
            installation_id: "install-1".to_owned(),
            kind: ResourceKind::Gateway,
            project_id: None,
            compatibility_fingerprint: "sha256:caddy".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:gateway".to_owned(),
            retention: RetentionClass::Disposable,
        })
        .expect("gateway metadata");

        reconstruct_owned_container(
            &ObservedContainer::new(ContainerId::new("stackctl-gateway"), metadata.labels()),
            metadata.installation_id(),
            metadata.schema_version(),
        )
        .expect("owned gateway")
    }
}
