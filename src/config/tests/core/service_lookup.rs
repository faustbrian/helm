use super::helpers::mysql_service;
use super::*;
#[test]
fn find_service_success() {
    let config = Config {
        schema_version: 1,
        container_prefix: Some("test".to_owned()),
        service: vec![mysql_service("db1"), mysql_service("db2")],
        swarm: vec![],
    };

    let result = find_service(&config, "db2");
    assert!(result.is_ok());
    assert_eq!(result.expect("result present").name, "db2");
}

#[test]
fn find_service_not_found() {
    let config = Config {
        schema_version: 1,
        container_prefix: Some("test".to_owned()),
        service: vec![mysql_service("db1")],
        swarm: vec![],
    };

    let result = find_service(&config, "nonexistent");
    assert!(result.is_err());
    let err = result.expect_err("error expected");
    assert!(err.to_string().contains("not found"));
    assert!(err.to_string().contains("db1"));
}

#[test]
fn find_service_empty_list() {
    let config = Config {
        schema_version: 1,
        container_prefix: Some("test".to_owned()),
        service: vec![],
        swarm: vec![],
    };

    let result = find_service(&config, "any");
    assert!(result.is_err());
    let err = result.expect_err("error expected");
    assert!(err.to_string().contains("none"));
}
