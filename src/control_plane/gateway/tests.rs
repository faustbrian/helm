use super::{
    CaddyGatewayProvider, GatewayConfiguration, GatewayDocumentLoader, GatewayError, GatewayFuture,
    GatewayRoute, GatewaySnapshot, render_caddy_document, store_caddy_bootstrap,
};
use serde_json::Value;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
use super::CaddyUnixAdminClient;

#[cfg(unix)]
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[cfg(unix)]
use tokio::net::UnixListener;

#[test]
fn complete_route_snapshots_are_sorted_before_provider_application() {
    let snapshot = GatewaySnapshot::new(
        "sha256:routes-v1",
        vec![
            GatewayRoute::new("shop-mailpit.stackctl.localhost", "http://mailpit:8025")
                .expect("mail route"),
            GatewayRoute::new("shop-app.stackctl.localhost", "http://shop-app:8080")
                .expect("app route"),
        ],
    )
    .expect("complete snapshot");

    assert_eq!(
        snapshot
            .routes()
            .iter()
            .map(|route| route.domain())
            .collect::<Vec<_>>(),
        vec![
            "shop-app.stackctl.localhost",
            "shop-mailpit.stackctl.localhost"
        ]
    );
    assert_eq!(snapshot.revision(), "sha256:routes-v1");
}

#[test]
fn duplicate_domains_reject_the_entire_gateway_snapshot() {
    let error = GatewaySnapshot::new(
        "sha256:routes-v1",
        vec![
            GatewayRoute::new("shop-app.stackctl.localhost", "http://shop-app:8080")
                .expect("first route"),
            GatewayRoute::new("shop-app.stackctl.localhost", "http://other-app:8080")
                .expect("second route"),
        ],
    )
    .expect_err("duplicate domain");

    assert_eq!(
        error.to_string(),
        "gateway domain 'shop-app.stackctl.localhost' has multiple upstreams"
    );
}

#[test]
fn gateway_upstreams_must_use_internal_plain_http() {
    let error = GatewayRoute::new("shop-app.stackctl.localhost", "https://shop-app:8443")
        .expect_err("TLS belongs at gateway");

    assert_eq!(
        error.to_string(),
        "gateway upstream 'https://shop-app:8443' must use internal plain HTTP"
    );
}

#[test]
fn gateway_configuration_is_an_object_safe_atomic_strategy() {
    let snapshot = GatewaySnapshot::new(
        "sha256:routes-v1",
        vec![
            GatewayRoute::new("shop-app.stackctl.localhost", "http://shop-app:8080")
                .expect("app route"),
        ],
    )
    .expect("complete snapshot");
    let mut provider = RecordingGatewayProvider::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    runtime
        .block_on(apply_through_strategy(&mut provider, &snapshot))
        .expect("atomic apply");

    assert_eq!(provider.applied, vec![snapshot]);
}

#[test]
fn caddy_document_uses_stackctl_tls_plain_upstreams_and_private_admin_socket() {
    let snapshot = GatewaySnapshot::new(
        "sha256:routes-v1",
        vec![
            GatewayRoute::new("shop-app.stackctl.localhost", "http://shop-app:8080")
                .expect("app route"),
        ],
    )
    .expect("complete snapshot");

    let document = render_caddy_document(
        &snapshot,
        Path::new("/etc/stackctl/tls/leaf.pem"),
        Path::new("/etc/stackctl/tls/leaf-key.pem"),
        Path::new("/run/stackctl/admin.sock"),
    )
    .expect("valid Caddy document");
    let json: Value = serde_json::from_slice(document.bytes()).expect("Caddy JSON");

    assert_eq!(
        json.pointer("/admin/listen").and_then(Value::as_str),
        Some("unix//run/stackctl/admin.sock|0600")
    );
    assert_eq!(
        json.pointer("/admin/config/persist")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        json.pointer("/apps/tls/certificates/load_files/0/certificate")
            .and_then(Value::as_str),
        Some("/etc/stackctl/tls/leaf.pem")
    );
    assert_eq!(
        json.pointer("/apps/http/servers/stackctl_https/routes/0/handle/0/upstreams/0/dial")
            .and_then(Value::as_str),
        Some("shop-app:8080")
    );
    assert_eq!(
        json.pointer("/apps/http/servers/stackctl_https/routes/0/handle/0/handler")
            .and_then(Value::as_str),
        Some("reverse_proxy")
    );
    assert_eq!(
        json.pointer("/apps/http/servers/stackctl_https/tls_connection_policies/0"),
        Some(&serde_json::json!({}))
    );
    assert!(json.pointer("/apps/pki").is_none());
    assert_eq!(document.revision(), snapshot.revision());
}

#[cfg(unix)]
#[test]
fn caddy_bootstrap_is_atomic_private_and_idempotent() {
    let root = std::env::temp_dir().join(format!(
        "stackctl-gateway-bootstrap-{}-{}",
        std::process::id(),
        unique_test_value()
    ));
    let config_path = root.join("config/config.json");
    let runtime_directory = root.join("run");
    let snapshot = GatewaySnapshot::new("sha256:routes-v1", Vec::new()).unwrap();
    let document = render_caddy_document(
        &snapshot,
        Path::new("/etc/stackctl/tls/leaf.pem"),
        Path::new("/etc/stackctl/tls/leaf-key.pem"),
        Path::new("/run/stackctl/admin.sock"),
    )
    .unwrap();

    let stored = store_caddy_bootstrap(&document, &config_path, &runtime_directory)
        .expect("store bootstrap");
    store_caddy_bootstrap(&document, &config_path, &runtime_directory).expect("repeat bootstrap");

    assert_eq!(stored.config_path(), config_path);
    assert_eq!(stored.runtime_directory(), runtime_directory);
    assert_eq!(
        stored.admin_socket_path(),
        runtime_directory.join("admin.sock")
    );
    assert_eq!(std::fs::read(&config_path).unwrap(), document.bytes());

    #[cfg(unix)]
    {
        assert_eq!(
            std::fs::metadata(&config_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&runtime_directory)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }

    std::fs::remove_dir_all(root).expect("remove bootstrap directory");
}

#[test]
fn caddy_provider_advances_revision_only_after_atomic_load_succeeds() {
    let snapshot = GatewaySnapshot::new(
        "sha256:routes-v1",
        vec![
            GatewayRoute::new("shop-app.stackctl.localhost", "http://shop-app:8080")
                .expect("app route"),
        ],
    )
    .expect("complete snapshot");
    let loader = RecordingDocumentLoader::default();
    let mut provider = CaddyGatewayProvider::new(
        loader,
        "/etc/stackctl/tls/leaf.pem",
        "/etc/stackctl/tls/leaf-key.pem",
        "/run/stackctl/admin.sock",
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    runtime
        .block_on(provider.apply_snapshot(&snapshot))
        .expect("atomic load");

    assert_eq!(
        runtime.block_on(provider.active_revision()).unwrap(),
        Some("sha256:routes-v1".to_owned())
    );
    assert_eq!(provider.loader().documents.len(), 1);
}

#[test]
fn caddy_provider_keeps_previous_revision_when_load_fails() {
    let snapshot = GatewaySnapshot::new("sha256:routes-v1", Vec::new()).unwrap();
    let loader = RecordingDocumentLoader {
        failure: Some("Caddy rejected configuration".to_owned()),
        ..RecordingDocumentLoader::default()
    };
    let mut provider = CaddyGatewayProvider::new(
        loader,
        "/etc/stackctl/tls/leaf.pem",
        "/etc/stackctl/tls/leaf-key.pem",
        "/run/stackctl/admin.sock",
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(provider.apply_snapshot(&snapshot))
        .expect_err("load failure");

    assert_eq!(error.to_string(), "Caddy rejected configuration");
    assert_eq!(runtime.block_on(provider.active_revision()).unwrap(), None);
}

#[cfg(unix)]
#[test]
fn caddy_admin_client_loads_complete_json_over_private_unix_http() {
    let socket_path = std::env::temp_dir().join(format!(
        "stackctl-caddy-admin-{}-{}.sock",
        std::process::id(),
        unique_test_value()
    ));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("test runtime");

    runtime.block_on(async {
        let listener = UnixListener::bind(&socket_path).expect("bind admin socket");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept request");
            let mut request = vec![0; 4096];
            let length = stream.read(&mut request).await.expect("read request");
            request.truncate(length);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await
                .expect("write response");
            request
        });
        let mut client = CaddyUnixAdminClient::new(&socket_path);

        client
            .load_document(br#"{"apps":{}}"#)
            .await
            .expect("atomic Caddy load");
        let request = server.await.expect("server task");
        let request = String::from_utf8(request).expect("HTTP request");

        assert!(request.starts_with("POST /load HTTP/1.1\r\n"));
        assert!(request.contains("Content-Type: application/json\r\n"));
        assert!(request.ends_with(r#"{"apps":{}}"#));
    });

    std::fs::remove_file(&socket_path).expect("remove admin socket");
}

fn unique_test_value() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos()
}

fn apply_through_strategy<'operation>(
    strategy: &'operation mut dyn GatewayConfiguration,
    snapshot: &'operation GatewaySnapshot,
) -> GatewayFuture<'operation, ()> {
    strategy.apply_snapshot(snapshot)
}

#[derive(Default)]
struct RecordingGatewayProvider {
    applied: Vec<GatewaySnapshot>,
}

#[derive(Default)]
struct RecordingDocumentLoader {
    documents: Vec<Vec<u8>>,
    failure: Option<String>,
}

impl GatewayDocumentLoader for RecordingDocumentLoader {
    fn load_document<'operation>(
        &'operation mut self,
        document: &'operation [u8],
    ) -> GatewayFuture<'operation, ()> {
        Box::pin(async move {
            if let Some(failure) = &self.failure {
                return Err(GatewayError::Provider {
                    detail: failure.clone(),
                });
            }

            self.documents.push(document.to_vec());
            Ok(())
        })
    }
}

impl GatewayConfiguration for RecordingGatewayProvider {
    fn apply_snapshot<'operation>(
        &'operation mut self,
        snapshot: &'operation GatewaySnapshot,
    ) -> GatewayFuture<'operation, ()> {
        Box::pin(async move {
            self.applied.push(snapshot.clone());

            Ok(())
        })
    }

    fn active_revision(&self) -> GatewayFuture<'_, Option<String>> {
        Box::pin(async { Ok(None) })
    }
}
