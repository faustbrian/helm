use super::{
    GotenbergPreparationOptions, prepare_gotenberg_shared_instances,
    reconcile_prepared_gotenberg_instance,
};
use crate::control_plane::engine::{
    AttachedCommandOptions, BollardEngineAdapter, CommandRequest, ContainerDiscovery, EngineError,
    ImageResolver, ImmutableImageReference, InstallationResourceDeletionOptions,
    ManagedResourceMetadata, ManagedResourceMetadataOptions, NetworkCreateOptions, NetworkManager,
    OwnedContainer, ResourceKind, RetentionClass, delete_owned_installation_resources,
    reconstruct_owned_container, run_attached_command_capture,
};
use crate::control_plane::shared_infrastructure::{
    CompatibilityFingerprintOptions, CompatibilityProfile, IsolationCapability, PersistenceMode,
    SharedServiceRequest, plan_shared_instances,
};
use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const GOTENBERG_IMAGE: &str = concat!(
    "gotenberg/gotenberg@sha256:",
    "67097317623a503ba2a6a7e9ae8db6929a1f7e1bbd88077bacf2d325fbdab923"
);

#[test]
#[ignore = "CI owns live shared Gotenberg conversion acceptance"]
fn live_docker_engine_two_projects_share_one_gotenberg_for_pdf_conversion() {
    let socket = std::env::var_os("STACKCTL_ENGINE_SOCKET")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/var/run/docker.sock"));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must follow the Unix epoch")
        .as_nanos();
    let installation_id = format!("ci-{}-{nonce}", std::process::id());
    let network_name = format!("stackctl-{installation_id}");
    let platform = match std::env::consts::ARCH {
        "aarch64" => "linux/arm64",
        "x86_64" => "linux/amd64",
        architecture => panic!("unsupported Gotenberg acceptance architecture '{architecture}'"),
    };
    let profile = CompatibilityProfile::from_options(CompatibilityFingerprintOptions {
        implementation: "gotenberg".to_owned(),
        major_version: "8".to_owned(),
        image_digest: GOTENBERG_IMAGE.to_owned(),
        extensions: Vec::new(),
        immutable_settings: BTreeMap::new(),
        persistence: PersistenceMode::Ephemeral,
        isolation: IsolationCapability::None,
        platform_architecture: Some(platform.to_owned()),
    })
    .expect("build Gotenberg acceptance profile");
    let shared = plan_shared_instances(vec![
        SharedServiceRequest::new("bill", "pdf", profile.clone()),
        SharedServiceRequest::new("shop", "pdf", profile),
    ]);
    assert_eq!(shared.len(), 1, "compatible projects must share one plan");
    assert_eq!(shared[0].consumers().len(), 2);
    let prepared = prepare_gotenberg_shared_instances(
        &shared,
        GotenbergPreparationOptions {
            installation_id: &installation_id,
            network_name: &network_name,
            schema_version: 8,
        },
    )
    .expect("prepare shared Gotenberg acceptance resources");
    let prepared = &prepared[0];
    assert_eq!(prepared.projects().len(), 2);
    assert!(prepared.projects().iter().all(|project| {
        project
            .environment()
            .values()
            .get("GOTENBERG_URL")
            .is_some_and(|url| url.ends_with(":3000"))
    }));

    let network_metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.clone(),
        kind: ResourceKind::Network,
        project_id: None,
        compatibility_fingerprint: shared[0].fingerprint().as_str().to_owned(),
        schema_version: 8,
        desired_revision: shared[0].fingerprint().as_str().to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("build Gotenberg acceptance network metadata");
    let network_request = NetworkCreateOptions::new(&network_name, network_metadata)
        .expect("build Gotenberg acceptance network request");
    let image = ImmutableImageReference::new(GOTENBERG_IMAGE)
        .expect("build immutable Gotenberg acceptance image reference");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build Gotenberg acceptance runtime");
    let mut engine = runtime
        .block_on(BollardEngineAdapter::connect_unix(&socket))
        .expect("negotiate the selected Docker Engine API");

    runtime.block_on(async {
        engine
            .ensure_image(&image)
            .await
            .expect("resolve immutable Gotenberg acceptance image");
        engine
            .create_network(&network_request)
            .await
            .expect("create private Gotenberg acceptance network");
        let first =
            reconcile_prepared_gotenberg_instance(&mut engine, prepared, &installation_id, 8)
                .await
                .expect("converge shared Gotenberg for two projects");
        assert!(first.logical_resource_drifts().is_empty());
        assert_eq!(first.logical_resources().len(), 2);
        let container = owned_shared_container(&engine, &installation_id).await;

        let bill_pdf = convert_html(
            &engine,
            &container,
            "<html><body>bill acceptance</body></html>",
        )
        .await
        .expect("convert bill HTML to PDF");
        assert!(bill_pdf.starts_with(b"%PDF-"));
        let shop_pdf = convert_html(
            &engine,
            &container,
            "<html><body>shop acceptance</body></html>",
        )
        .await
        .expect("convert shop HTML to PDF");
        assert!(shop_pdf.starts_with(b"%PDF-"));

        let second =
            reconcile_prepared_gotenberg_instance(&mut engine, prepared, &installation_id, 8)
                .await
                .expect("reconcile unchanged shared Gotenberg instance");
        assert!(second.logical_resource_drifts().is_empty());
        let replayed_container = owned_shared_container(&engine, &installation_id).await;
        assert_eq!(replayed_container.id(), container.id());

        delete_owned_installation_resources(
            &mut engine,
            InstallationResourceDeletionOptions {
                installation_id: &installation_id,
                schema_version: 8,
                authorized_persistent_volumes: &[],
            },
        )
        .await
        .expect("delete shared Gotenberg acceptance resources");
    });

    println!("shared Gotenberg conversion acceptance passed for {installation_id}");
}

async fn owned_shared_container(
    engine: &BollardEngineAdapter,
    installation_id: &str,
) -> OwnedContainer {
    engine
        .discover_managed()
        .await
        .expect("discover shared Gotenberg acceptance resources")
        .into_iter()
        .filter_map(|observed| reconstruct_owned_container(&observed, installation_id, 8).ok())
        .find(|owned| owned.metadata().kind() == ResourceKind::SharedService)
        .expect("discover one owned shared Gotenberg container")
}

async fn convert_html(
    engine: &BollardEngineAdapter,
    container: &OwnedContainer,
    html: &str,
) -> Result<Vec<u8>, EngineError> {
    let request = CommandRequest::new(
        vec![
            "curl".to_owned(),
            "--fail".to_owned(),
            "--silent".to_owned(),
            "--show-error".to_owned(),
            "--form".to_owned(),
            "files=@-;filename=index.html;type=text/html".to_owned(),
            "http://127.0.0.1:3000/forms/chromium/convert/html".to_owned(),
        ],
        BTreeMap::new(),
        None,
    )?;
    let options = AttachedCommandOptions::new(
        request,
        html.as_bytes().to_vec(),
        "exercise Gotenberg HTML conversion",
        Duration::from_secs(30),
    )?;

    run_attached_command_capture(engine, container, &options).await
}
