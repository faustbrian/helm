use super::{ApplicationContainerPlan, ApplicationContainerPlanOptions};
use crate::control_plane::ProjectIdentity;
use std::path::{Path, PathBuf};

#[test]
fn project_applications_use_private_networking_without_host_ports() {
    let plan = application_plan("bill", "/work/bill");

    assert_eq!(plan.container_name(), "stackctl-bill-app");
    assert_eq!(plan.network_name(), "stackctl-private");
    assert_eq!(plan.source_path(), Path::new("/work/bill"));
    assert_eq!(plan.internal_http_port(), 8080);
    assert!(plan.published_ports().is_empty());
    assert_eq!(plan.gateway_route().domain(), "bill-app.stackctl.localhost");
    assert_eq!(
        plan.gateway_route().upstream(),
        "http://stackctl-bill-app:8080"
    );
}

#[test]
fn equal_runtime_images_still_produce_dedicated_project_containers() {
    let bill = application_plan("bill", "/work/bill");
    let shop = application_plan("shop", "/work/shop");

    assert_eq!(bill.image_digest(), shop.image_digest());
    assert_ne!(bill.container_name(), shop.container_name());
    assert_ne!(bill.source_path(), shop.source_path());
}

#[test]
fn mutable_application_images_fail_before_engine_planning() {
    let mut options = application_options("bill", "/work/bill");
    options.image_digest = "ghcr.io/stackctl/php:8.4".to_owned();

    let error = ApplicationContainerPlan::new(options).expect_err("mutable image");

    assert_eq!(
        error.to_string(),
        "application image 'ghcr.io/stackctl/php:8.4' must use an immutable sha256 digest"
    );
}

fn application_plan(project: &str, path: &str) -> ApplicationContainerPlan {
    ApplicationContainerPlan::new(application_options(project, path))
        .expect("valid application plan")
}

fn application_options(project: &str, path: &str) -> ApplicationContainerPlanOptions {
    ApplicationContainerPlanOptions {
        project: ProjectIdentity::resolve(Some(project), Path::new(path))
            .expect("project identity"),
        image_digest: concat!(
            "ghcr.io/stackctl/php@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )
        .to_owned(),
        source_path: PathBuf::from(path),
        network_name: "stackctl-private".to_owned(),
        internal_http_port: 8080,
    }
}
