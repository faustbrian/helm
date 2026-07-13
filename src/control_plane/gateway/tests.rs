use super::{GatewayConfiguration, GatewayFuture, GatewayRoute, GatewaySnapshot};

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
        .build()
        .expect("test runtime");

    runtime
        .block_on(apply_through_strategy(&mut provider, &snapshot))
        .expect("atomic apply");

    assert_eq!(provider.applied, vec![snapshot]);
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
