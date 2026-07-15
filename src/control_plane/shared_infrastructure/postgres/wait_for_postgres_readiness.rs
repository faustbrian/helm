use super::POSTGRES_BOOTSTRAP_USERNAME;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, OwnedContainer,
    run_attached_command,
};
use crate::control_plane::state::{CredentialLifecycle, CredentialRecord};
use std::collections::BTreeMap;
use std::time::Duration;

const READINESS_ATTEMPTS: usize = 30;
const READINESS_RETRY_MILLISECONDS: u64 = 200;
const READINESS_PROBE_TIMEOUT_SECONDS: u64 = 5;

/// Waits for PostgreSQL through its managed administrator identity.
pub(super) async fn wait_for_postgres_readiness(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    administrator: &CredentialRecord,
) -> Result<(), EngineError> {
    if administrator.username() != POSTGRES_BOOTSTRAP_USERNAME
        || administrator.secret().is_empty()
        || administrator.lifecycle() != CredentialLifecycle::Active
    {
        return Err(EngineError::InvalidRequest {
            detail: "PostgreSQL readiness requires the active bootstrap administrator".to_owned(),
        });
    }
    let request = CommandRequest::new(
        vec![
            "psql".to_owned(),
            "--no-psqlrc".to_owned(),
            "--set=ON_ERROR_STOP=1".to_owned(),
            format!("--username={POSTGRES_BOOTSTRAP_USERNAME}"),
            "--dbname=postgres".to_owned(),
            "--command=SELECT 1".to_owned(),
        ],
        BTreeMap::from([("PGPASSWORD".to_owned(), administrator.secret().to_owned())]),
        None,
    )?;
    let options = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "wait for PostgreSQL readiness",
        Duration::from_secs(READINESS_PROBE_TIMEOUT_SECONDS),
    )?;

    for attempt in 1..=READINESS_ATTEMPTS {
        match run_attached_command(executor, container, &options).await {
            Ok(()) => return Ok(()),
            Err(EngineError::ContainerExit { .. } | EngineError::Backend { .. })
                if attempt < READINESS_ATTEMPTS =>
            {
                tokio::time::sleep(Duration::from_millis(READINESS_RETRY_MILLISECONDS)).await;
            }
            Err(error) => return Err(error),
        }
    }

    unreachable!("the final PostgreSQL readiness attempt always returns")
}

#[cfg(test)]
mod tests {
    use super::wait_for_postgres_readiness;
    use crate::control_plane::engine::{
        CommandExecutionId, CommandExecutor, CommandRequest, CommandSession, CommandStatus,
        ContainerId, ContainerLogStream, EngineFuture, ManagedResourceMetadata,
        ManagedResourceMetadataOptions, ObservedContainer, OwnedContainer, ResourceKind,
        RetentionClass, reconstruct_owned_container,
    };
    use crate::control_plane::state::{
        CredentialLifecycle, CredentialRecord, CredentialRecordOptions,
    };
    use futures_util::stream;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::io::AsyncReadExt;

    #[test]
    fn readiness_retries_transient_exit_with_environment_authentication() {
        let executor = SequencedExecutor::new([CommandStatus::Exited(2), CommandStatus::Exited(0)]);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("build PostgreSQL readiness test runtime");

        runtime
            .block_on(wait_for_postgres_readiness(
                &executor,
                &owned_container(),
                &administrator(),
            ))
            .expect("wait for PostgreSQL after transient startup exit");

        let requests = executor.requests.lock().expect("PostgreSQL requests");
        assert_eq!(requests.len(), 2);
        assert!(
            requests
                .iter()
                .all(|(_, debug)| debug.contains("PGPASSWORD"))
        );
        assert!(
            requests
                .iter()
                .all(|(_, debug)| !debug.contains("admin-secret"))
        );
        assert!(requests.iter().all(
            |(arguments, _)| arguments.last().map(String::as_str) == Some("--command=SELECT 1")
        ));
    }

    fn owned_container() -> OwnedContainer {
        let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
            installation_id: "install-1".to_owned(),
            kind: ResourceKind::SharedService,
            project_id: None,
            compatibility_fingerprint: "sha256:postgres-18".to_owned(),
            schema_version: 8,
            desired_revision: "sha256:desired-v1".to_owned(),
            retention: RetentionClass::Persistent,
        })
        .expect("build PostgreSQL readiness metadata");
        let observed =
            ObservedContainer::new(ContainerId::new("postgres-container"), metadata.labels());

        reconstruct_owned_container(&observed, "install-1", 8)
            .expect("build owned PostgreSQL readiness container")
    }

    fn administrator() -> CredentialRecord {
        CredentialRecord::new(CredentialRecordOptions {
            credential_id: "shared/postgres/bootstrap".to_owned(),
            project_id: None,
            service_id: "postgresql".to_owned(),
            username: "stackctl_admin".to_owned(),
            secret: "admin-secret".to_owned(),
            lifecycle: CredentialLifecycle::Active,
        })
    }

    struct SequencedExecutor {
        requests: Arc<Mutex<Vec<(Vec<String>, String)>>>,
        statuses: Arc<Mutex<VecDeque<CommandStatus>>>,
        next_execution: AtomicUsize,
    }

    impl SequencedExecutor {
        fn new(statuses: impl IntoIterator<Item = CommandStatus>) -> Self {
            Self {
                requests: Arc::new(Mutex::new(Vec::new())),
                statuses: Arc::new(Mutex::new(statuses.into_iter().collect())),
                next_execution: AtomicUsize::new(1),
            }
        }
    }

    impl CommandExecutor for SequencedExecutor {
        fn start_command<'operation>(
            &'operation self,
            container: &'operation OwnedContainer,
            request: &'operation CommandRequest,
        ) -> EngineFuture<'operation, CommandSession> {
            self.requests
                .lock()
                .expect("PostgreSQL request lock")
                .push((request.arguments().to_vec(), format!("{request:?}")));
            let execution = self.next_execution.fetch_add(1, Ordering::SeqCst);
            let container_id = container.id().clone();

            Box::pin(async move {
                let (writer, mut reader) = tokio::io::duplex(1024);
                tokio::spawn(async move {
                    let mut bytes = Vec::new();
                    reader
                        .read_to_end(&mut bytes)
                        .await
                        .expect("read PostgreSQL readiness stdin");
                });
                let output: ContainerLogStream<'static> = Box::pin(stream::empty());

                Ok(CommandSession::new(
                    CommandExecutionId::new(format!("postgres-exec-{execution}")),
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
            Box::pin(async move {
                self.statuses
                    .lock()
                    .expect("PostgreSQL status lock")
                    .pop_front()
                    .ok_or_else(|| crate::control_plane::engine::EngineError::Backend {
                        detail: "PostgreSQL test executor has no configured status".to_owned(),
                    })
            })
        }
    }
}
