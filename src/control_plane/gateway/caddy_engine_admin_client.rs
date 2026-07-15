use super::{GatewayDocumentLoader, GatewayError, GatewayFuture};
use crate::control_plane::engine::{
    CommandExecutor, CommandRequest, EngineError, OwnedContainer, StreamingCommandOptions,
    run_streaming_command,
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
            .map_err(reload_error)
        })
    }
}

fn reload_error(error: EngineError) -> GatewayError {
    match error {
        EngineError::Backend { .. } | EngineError::Timeout { .. } => GatewayError::Engine {
            action: "admin reload".to_owned(),
            detail: error.to_string(),
        },
        EngineError::InvalidRequest { .. }
        | EngineError::ContainerExit { .. }
        | EngineError::OwnershipMismatch { .. } => provider_error(error),
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
        EngineError, EngineFuture, LogChunk, ManagedResourceMetadata,
        ManagedResourceMetadataOptions, ObservedContainer, ResourceKind, RetentionClass,
        reconstruct_owned_container,
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

    #[test]
    fn preserves_engine_transport_failures_for_runtime_reconnect() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("test runtime");
        let mut client =
            CaddyEngineAdminClient::new(FailingExecutor, owned_gateway(), Duration::from_secs(5));

        let error = runtime
            .block_on(client.load_document(br#"{"apps":{}}"#))
            .expect_err("Engine transport failure");

        assert_eq!(
            error,
            GatewayError::Engine {
                action: "admin reload".to_owned(),
                detail: "Engine socket unavailable".to_owned(),
            }
        );
    }

    #[test]
    fn reports_caddy_command_exit_as_provider_failure() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("test runtime");
        let executor = RecordingExecutor {
            exit_status: 9,
            ..RecordingExecutor::default()
        };
        let mut client =
            CaddyEngineAdminClient::new(executor, owned_gateway(), Duration::from_secs(5));

        let error = runtime
            .block_on(client.load_document(br#"{"apps":{}}"#))
            .expect_err("Caddy command failure");

        assert_eq!(
            error,
            GatewayError::Provider {
                detail: "Caddy Engine admin reload failed: container 'stackctl-gateway' exited with status 9"
                    .to_owned(),
            }
        );
    }

    #[derive(Clone, Default)]
    struct RecordingExecutor {
        arguments: Arc<Mutex<Vec<String>>>,
        input: Arc<Mutex<Vec<u8>>>,
        exit_status: i64,
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
            Box::pin(async move { Ok(CommandStatus::Exited(self.exit_status)) })
        }
    }

    struct FailingExecutor;

    impl CommandExecutor for FailingExecutor {
        fn start_command<'operation>(
            &'operation self,
            _container: &'operation OwnedContainer,
            _request: &'operation CommandRequest,
        ) -> EngineFuture<'operation, CommandSession> {
            Box::pin(async {
                Err(EngineError::Backend {
                    detail: "Engine socket unavailable".to_owned(),
                })
            })
        }

        fn command_status<'operation>(
            &'operation self,
            _execution_id: &'operation CommandExecutionId,
            _container_id: &'operation ContainerId,
        ) -> EngineFuture<'operation, CommandStatus> {
            Box::pin(async { unreachable!("command never starts") })
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
