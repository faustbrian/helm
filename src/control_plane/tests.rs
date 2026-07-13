use super::{ProjectIdentity, RouteIdentity, ServiceIdentity};
use std::path::Path;

#[test]
fn explicit_project_name_is_preserved_exactly() {
    let project = ProjectIdentity::resolve(Some("bill-1"), Path::new("/work/bill"))
        .expect("valid explicit project name");

    assert_eq!(project.as_str(), "bill-1");
}

#[test]
fn directory_basename_is_used_when_project_name_is_absent() {
    let project = ProjectIdentity::resolve(None, Path::new("/work/bill-2"))
        .expect("valid directory project name");

    assert_eq!(project.as_str(), "bill-2");
}

#[test]
fn project_name_is_rejected_instead_of_normalized() {
    let error = ProjectIdentity::resolve(Some("Billing_API"), Path::new("/work/bill"))
        .expect_err("invalid project name must fail");

    assert_eq!(
        error.to_string(),
        "project name 'Billing_API' must be a valid lowercase DNS label"
    );
}

#[test]
fn service_name_is_rejected_instead_of_normalized() {
    let error = ServiceIdentity::new("Mail Pit").expect_err("invalid service name must fail");

    assert_eq!(
        error.to_string(),
        "service name 'Mail Pit' must be a valid lowercase DNS label"
    );
}

#[test]
fn every_route_includes_the_project_and_service_names() {
    let project = ProjectIdentity::resolve(Some("bill"), Path::new("/work/ignored"))
        .expect("valid project name");
    let service = ServiceIdentity::new("app").expect("valid service name");
    let route = RouteIdentity::new(&project, &service).expect("valid route");

    assert_eq!(route.domain(), "bill-app.stackctl.localhost");
}

#[test]
fn combined_route_label_longer_than_63_bytes_is_rejected() {
    let project = ProjectIdentity::resolve(
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        Path::new("/work/ignored"),
    )
    .expect("valid project name");
    let service = ServiceIdentity::new("bbbbbbbbbbbbbbbbbbbbbbbb").expect("valid service name");

    let error = RouteIdentity::new(&project, &service).expect_err("route label must be bounded");

    assert_eq!(
        error.to_string(),
        "route label 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-bbbbbbbbbbbbbbbbbbbbbbbb' exceeds 63 bytes"
    );
}
