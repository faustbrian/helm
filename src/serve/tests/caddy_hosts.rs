use super::super::caddy::{render_caddyfile, served_url};
use super::super::hosts::hosts_file_has_domain;
use super::super::{CaddyPorts, CaddyState};
use std::collections::BTreeMap;
use std::path::Path;

#[test]
fn caddyfile_contains_domain_and_upstream() {
    let mut routes = BTreeMap::new();
    routes.insert("donkey.helm".to_owned(), "127.0.0.1:8080".to_owned());
    let state = CaddyState { routes };
    let ports = CaddyPorts {
        http: 80,
        https: 443,
    };

    let rendered = render_caddyfile(&state, ports, Path::new("/tmp/access.log"));
    assert!(rendered.contains("donkey.helm"));
    assert!(rendered.contains("reverse_proxy 127.0.0.1:8080"));
    assert!(rendered.contains("tls internal"));
}

#[test]
fn hosts_parser_detects_domain() {
    let content = "127.0.0.1 localhost donkey.helm\n";
    assert!(hosts_file_has_domain(content, "donkey.helm"));
}

#[test]
fn hosts_parser_ignores_comments() {
    let content = "# 127.0.0.1 donkey.helm\n127.0.0.1 localhost\n";
    assert!(!hosts_file_has_domain(content, "donkey.helm"));
}

#[test]
fn served_url_hides_default_https_port() {
    assert_eq!(
        served_url("acme-api.helm", 443),
        "https://acme-api.helm"
    );
}

#[test]
fn served_url_includes_non_default_https_port() {
    assert_eq!(
        served_url("acme-api.helm", 8443),
        "https://acme-api.helm:8443"
    );
}
