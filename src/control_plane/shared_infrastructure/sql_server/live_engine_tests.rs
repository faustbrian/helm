use super::{
    SqlServerPreparationOptions, prepare_sql_server_shared_instances,
    reconcile_prepared_sql_server_instance,
};
use crate::control_plane::engine::{
    AttachedCommandOptions, BollardEngineAdapter, CommandRequest, ContainerDiscovery, EngineError,
    ImageResolver, ImmutableImageReference, InstallationResourceDeletionOptions,
    ManagedResourceMetadata, ManagedResourceMetadataOptions, NetworkCreateOptions, NetworkManager,
    OwnedContainer, ResourceKind, RetentionClass, delete_owned_installation_resources,
    reconstruct_owned_container, run_attached_command_capture,
};
use crate::control_plane::shared_infrastructure::{
    CompatibilityFingerprintOptions, CompatibilityProfile, CredentialEntropy,
    CredentialGenerationError, IsolationCapability, OrphanedSharedAccessOptions, PersistenceMode,
    SharedServiceRequest, plan_shared_instances, revoke_orphaned_shared_access_from_observed,
};
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
    LogicalResourceRecordOptions, ResourceLifecycle, SqliteStateStore,
};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SQL_SERVER_IMAGE: &str = concat!(
    "mcr.microsoft.com/mssql/server@sha256:",
    "e07b9699a2b749969f19d86563ceeea22bd3a69f7f1db85a8d1ac4bdaf0c6f56"
);
const SQLCMD_PATH: &str = "/opt/mssql-tools18/bin/sqlcmd";

#[test]
#[ignore = "amd64 CI owns live shared SQL Server lifecycle acceptance"]
fn live_amd64_docker_engine_two_projects_share_one_sql_server_with_isolated_databases() {
    let socket = std::env::var_os("STACKCTL_ENGINE_SOCKET")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/var/run/docker.sock"));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must follow the Unix epoch")
        .as_nanos();
    let installation_id = format!("ci-{}-{nonce}", std::process::id());
    let network_name = format!("stackctl-{installation_id}");
    let state_directory = std::env::temp_dir().join(format!("stackctl-{installation_id}"));
    let profile = CompatibilityProfile::from_options(CompatibilityFingerprintOptions {
        implementation: "sqlserver".to_owned(),
        major_version: "2022".to_owned(),
        image_digest: SQL_SERVER_IMAGE.to_owned(),
        extensions: Vec::new(),
        immutable_settings: BTreeMap::from([("edition".to_owned(), "Developer".to_owned())]),
        persistence: PersistenceMode::Persistent,
        isolation: IsolationCapability::DatabaseAndRole,
        platform_architecture: Some("linux/amd64".to_owned()),
    })
    .expect("build SQL Server acceptance profile");
    let shared = plan_shared_instances(vec![
        SharedServiceRequest::new("bill", "database", profile.clone()),
        SharedServiceRequest::new("shop", "database", profile),
    ]);
    assert_eq!(shared.len(), 1, "compatible projects must share one plan");
    assert_eq!(shared[0].consumers().len(), 2);
    std::fs::create_dir(&state_directory).expect("create SQL Server acceptance state");
    let mut store = SqliteStateStore::open(&state_directory.join("state.sqlite3"))
        .expect("open SQL Server acceptance state store");
    let prepared = prepare_sql_server_shared_instances(
        &mut store,
        &shared,
        &SequentialCredentialEntropy::new(0x27),
        SqlServerPreparationOptions {
            installation_id: &installation_id,
            network_name: &network_name,
            schema_version: 8,
        },
    )
    .expect("prepare shared SQL Server acceptance resources");
    let replayed = prepare_sql_server_shared_instances(
        &mut store,
        &shared,
        &SequentialCredentialEntropy::new(0x67),
        SqlServerPreparationOptions {
            installation_id: &installation_id,
            network_name: &network_name,
            schema_version: 8,
        },
    )
    .expect("replay shared SQL Server acceptance preparation");
    assert_eq!(
        prepared[0].instance().bootstrap_credential(),
        replayed[0].instance().bootstrap_credential()
    );
    let prepared_credentials = prepared[0]
        .projects()
        .iter()
        .map(|project| project.credential().clone())
        .collect::<Vec<_>>();
    let replayed_credentials = replayed[0]
        .projects()
        .iter()
        .map(|project| project.credential().clone())
        .collect::<Vec<_>>();
    assert_eq!(prepared_credentials, replayed_credentials);
    assert_ne!(
        prepared_credentials[0].secret(),
        prepared_credentials[1].secret()
    );
    let prepared = &prepared[0];
    let bill = prepared
        .projects()
        .iter()
        .find(|project| project.environment().project_id() == "bill")
        .expect("prepared bill SQL Server resources");
    let shop = prepared
        .projects()
        .iter()
        .find(|project| project.environment().project_id() == "shop")
        .expect("prepared shop SQL Server resources");
    let network_metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.clone(),
        kind: ResourceKind::Network,
        project_id: None,
        compatibility_fingerprint: shared[0].fingerprint().as_str().to_owned(),
        schema_version: 8,
        desired_revision: shared[0].fingerprint().as_str().to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("build SQL Server acceptance network metadata");
    let network_request = NetworkCreateOptions::new(&network_name, network_metadata)
        .expect("build SQL Server acceptance network request");
    let image = ImmutableImageReference::new(SQL_SERVER_IMAGE)
        .expect("build immutable SQL Server acceptance image reference");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build SQL Server acceptance runtime");
    let mut engine = runtime
        .block_on(BollardEngineAdapter::connect_unix(&socket))
        .expect("negotiate the selected Docker Engine API");

    runtime.block_on(async {
        engine
            .ensure_image(&image)
            .await
            .expect("resolve immutable SQL Server acceptance image");
        engine
            .create_network(&network_request)
            .await
            .expect("create private SQL Server acceptance network");
        let first =
            reconcile_prepared_sql_server_instance(&mut engine, prepared, &installation_id, 8)
                .await
                .expect("converge shared SQL Server for two projects");
        assert!(
            first.logical_resource_drifts().is_empty(),
            "initial SQL Server convergence reported drift: {:?}",
            first.logical_resource_drifts()
        );
        assert_eq!(first.logical_resources().len(), 2);
        let container = owned_shared_container(&engine, &installation_id).await;

        let bill_output = sql_server_command(
            &engine,
            &container,
            bill.credential().username(),
            bill.credential().secret(),
            bill.logical().database_name(),
            "IF OBJECT_ID(N'stackctl_acceptance', N'U') IS NULL \
             CREATE TABLE stackctl_acceptance(value nvarchar(64)); \
             DELETE FROM stackctl_acceptance; \
             INSERT INTO stackctl_acceptance VALUES (N'bill-value'); \
             SELECT value FROM stackctl_acceptance;",
        )
        .await
        .expect("write and read bill SQL Server database");
        assert_sql_value(&bill_output, "bill-value");
        let shop_output = sql_server_command(
            &engine,
            &container,
            shop.credential().username(),
            shop.credential().secret(),
            shop.logical().database_name(),
            "IF OBJECT_ID(N'stackctl_acceptance', N'U') IS NULL \
             CREATE TABLE stackctl_acceptance(value nvarchar(64)); \
             DELETE FROM stackctl_acceptance; \
             INSERT INTO stackctl_acceptance VALUES (N'shop-value'); \
             SELECT value FROM stackctl_acceptance;",
        )
        .await
        .expect("write and read shop SQL Server database");
        assert_sql_value(&shop_output, "shop-value");
        assert!(matches!(
            sql_server_command(
                &engine,
                &container,
                shop.credential().username(),
                shop.credential().secret(),
                bill.logical().database_name(),
                "SELECT 1;",
            )
            .await,
            Err(EngineError::ContainerExit { .. })
        ));

        let second =
            reconcile_prepared_sql_server_instance(&mut engine, prepared, &installation_id, 8)
                .await
                .expect("reconcile unchanged shared SQL Server instance");
        assert!(second.logical_resource_drifts().is_empty());
        let replayed_container = owned_shared_container(&engine, &installation_id).await;
        assert_eq!(replayed_container.id(), container.id());

        let bill_logical = first
            .logical_resources()
            .iter()
            .find(|logical| logical.project_id() == "bill")
            .map(orphaned_logical_resource)
            .expect("find bill logical SQL Server resource");
        let credentials = [
            disabled_credential(bill.credential()),
            prepared.instance().bootstrap_credential().clone(),
        ];
        let observed = engine
            .discover_managed()
            .await
            .expect("discover SQL Server lifecycle acceptance resources");
        let revoked = revoke_orphaned_shared_access_from_observed(
            &mut engine,
            &observed,
            OrphanedSharedAccessOptions {
                resources: first.physical_resources(),
                logical_resources: std::slice::from_ref(&bill_logical),
                credentials: &credentials,
                installation_id: &installation_id,
                schema_version: 8,
                timeout: Duration::from_secs(30),
            },
        )
        .await
        .expect("revoke removed bill SQL Server access");
        assert_eq!(revoked, 1);
        assert!(matches!(
            sql_server_command(
                &engine,
                &container,
                bill.credential().username(),
                bill.credential().secret(),
                bill.logical().database_name(),
                "SELECT value FROM stackctl_acceptance;",
            )
            .await,
            Err(EngineError::ContainerExit { .. })
        ));
        assert_sql_value(
            &sql_server_command(
                &engine,
                &container,
                shop.credential().username(),
                shop.credential().secret(),
                shop.logical().database_name(),
                "SELECT value FROM stackctl_acceptance;",
            )
            .await
            .expect("read shop SQL Server data after bill removal"),
            "shop-value",
        );
        assert_sql_value(
            &sql_server_command(
                &engine,
                &container,
                prepared.instance().bootstrap_credential().username(),
                prepared.instance().bootstrap_credential().secret(),
                bill.logical().database_name(),
                "SELECT value FROM stackctl_acceptance;",
            )
            .await
            .expect("verify retained bill SQL Server data as administrator"),
            "bill-value",
        );

        let restored =
            reconcile_prepared_sql_server_instance(&mut engine, prepared, &installation_id, 8)
                .await
                .expect("restore bill SQL Server access from active configuration");
        assert!(restored.logical_resource_drifts().is_empty());
        assert_sql_value(
            &sql_server_command(
                &engine,
                &container,
                bill.credential().username(),
                bill.credential().secret(),
                bill.logical().database_name(),
                "SELECT value FROM stackctl_acceptance;",
            )
            .await
            .expect("read restored bill SQL Server data"),
            "bill-value",
        );
        let restored_container = owned_shared_container(&engine, &installation_id).await;
        assert_eq!(restored_container.id(), container.id());

        delete_owned_installation_resources(
            &mut engine,
            InstallationResourceDeletionOptions {
                installation_id: &installation_id,
                schema_version: 8,
                authorized_persistent_volumes: &[prepared
                    .instance()
                    .volume()
                    .expect("persistent SQL Server volume")
                    .name()
                    .to_owned()],
            },
        )
        .await
        .expect("delete shared SQL Server acceptance resources");
    });

    drop(store);
    std::fs::remove_dir_all(&state_directory).expect("remove SQL Server acceptance state");
    println!("shared SQL Server lifecycle acceptance passed for {installation_id}");
}

fn orphaned_logical_resource(logical: &LogicalResourceRecord) -> LogicalResourceRecord {
    LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: logical.logical_resource_id().to_owned(),
        shared_resource_id: logical.shared_resource_id().to_owned(),
        project_id: logical.project_id().to_owned(),
        service_id: logical.service_id().to_owned(),
        kind: logical.kind().to_owned(),
        compatibility_fingerprint: logical.compatibility_fingerprint().to_owned(),
        desired_revision: logical.desired_revision().to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(12_345),
    })
}

fn disabled_credential(credential: &CredentialRecord) -> CredentialRecord {
    CredentialRecord::new(CredentialRecordOptions {
        credential_id: credential.credential_id().to_owned(),
        project_id: credential.project_id().map(str::to_owned),
        service_id: credential.service_id().to_owned(),
        username: credential.username().to_owned(),
        secret: credential.secret().to_owned(),
        lifecycle: CredentialLifecycle::Disabled,
    })
}

fn assert_sql_value(output: &[u8], expected: &str) {
    assert_eq!(String::from_utf8_lossy(output).trim(), expected);
}

async fn owned_shared_container(
    engine: &BollardEngineAdapter,
    installation_id: &str,
) -> OwnedContainer {
    engine
        .discover_managed()
        .await
        .expect("discover shared SQL Server acceptance resources")
        .into_iter()
        .filter_map(|observed| reconstruct_owned_container(&observed, installation_id, 8).ok())
        .find(|owned| owned.metadata().kind() == ResourceKind::SharedService)
        .expect("discover one owned shared SQL Server container")
}

async fn sql_server_command(
    engine: &BollardEngineAdapter,
    container: &OwnedContainer,
    username: &str,
    password: &str,
    database: &str,
    sql: &str,
) -> Result<Vec<u8>, EngineError> {
    let sql = format!("SET NOCOUNT ON; {sql}");
    let request = CommandRequest::new(
        vec![
            SQLCMD_PATH.to_owned(),
            "-b".to_owned(),
            "-C".to_owned(),
            "-S".to_owned(),
            "127.0.0.1".to_owned(),
            "-U".to_owned(),
            username.to_owned(),
            "-d".to_owned(),
            database.to_owned(),
            "-h".to_owned(),
            "-1".to_owned(),
            "-W".to_owned(),
            "-Q".to_owned(),
            sql,
        ],
        BTreeMap::from([("SQLCMDPASSWORD".to_owned(), password.to_owned())]),
        None,
    )?;
    let options = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "exercise SQL Server tenant isolation",
        Duration::from_secs(30),
    )?;

    run_attached_command_capture(engine, container, &options).await
}

struct SequentialCredentialEntropy {
    next: AtomicU8,
}

impl SequentialCredentialEntropy {
    const fn new(first: u8) -> Self {
        Self {
            next: AtomicU8::new(first),
        }
    }
}

impl CredentialEntropy for SequentialCredentialEntropy {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), CredentialGenerationError> {
        bytes.fill(self.next.fetch_add(1, Ordering::SeqCst));

        Ok(())
    }
}
