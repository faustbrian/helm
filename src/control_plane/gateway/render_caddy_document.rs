use super::{CaddyGatewayDocument, GatewayError, GatewayRoute, GatewaySnapshot};
use serde_json::{Value, json};
use std::path::Path;

/// Renders one complete native Caddy configuration with Stackctl-owned TLS.
pub(crate) fn render_caddy_document(
    snapshot: &GatewaySnapshot,
    certificate_path: &Path,
    private_key_path: &Path,
    admin_address: &str,
) -> Result<CaddyGatewayDocument, GatewayError> {
    let certificate_path = absolute_utf8_path("certificate", certificate_path)?;
    let private_key_path = absolute_utf8_path("private key", private_key_path)?;
    if admin_address != "localhost:2019" {
        return Err(GatewayError::InvalidPlan {
            detail: "gateway admin address must remain private inside the container".to_owned(),
        });
    }
    let proxy_routes = snapshot
        .routes()
        .iter()
        .map(caddy_proxy_route)
        .collect::<Vec<_>>();
    let redirect_routes = snapshot
        .routes()
        .iter()
        .map(caddy_https_redirect_route)
        .collect::<Vec<_>>();
    let document = json!({
        "admin": {
            "listen": admin_address,
            "config": { "persist": false }
        },
        "apps": {
            "tls": {
                "certificates": {
                    "load_files": [{
                        "certificate": certificate_path,
                        "key": private_key_path,
                        "format": "pem"
                    }]
                }
            },
            "http": {
                "grace_period": "30s",
                "servers": {
                    "stackctl_http": {
                        "listen": [":80"],
                        "protocols": ["h1"],
                        "routes": redirect_routes
                    },
                    "stackctl_https": {
                        "listen": [":443"],
                        "protocols": ["h1", "h2"],
                        "routes": proxy_routes,
                        "tls_connection_policies": [{}]
                    }
                }
            }
        }
    });
    let bytes = serde_json::to_vec(&document).map_err(|error| GatewayError::InvalidPlan {
        detail: format!("failed to serialize Caddy gateway document: {error}"),
    })?;

    Ok(CaddyGatewayDocument::new(
        snapshot.revision().to_owned(),
        bytes,
    ))
}

fn caddy_proxy_route(route: &GatewayRoute) -> Value {
    json!({
        "match": [{ "host": [route.domain()] }],
        "handle": [{
            "handler": "reverse_proxy",
            "upstreams": [{ "dial": route.upstream().trim_start_matches("http://") }],
            "stream_close_delay": "5m"
        }],
        "terminal": true
    })
}

fn caddy_https_redirect_route(route: &GatewayRoute) -> Value {
    json!({
        "match": [{ "host": [route.domain()] }],
        "handle": [{
            "handler": "static_response",
            "status_code": 308,
            "headers": {
                "Location": ["https://{http.request.host}{http.request.uri}"]
            }
        }],
        "terminal": true
    })
}

fn absolute_utf8_path<'path>(kind: &str, path: &'path Path) -> Result<&'path str, GatewayError> {
    if !path.is_absolute() {
        return Err(GatewayError::InvalidPlan {
            detail: format!("gateway {kind} path '{}' must be absolute", path.display()),
        });
    }

    path.to_str().ok_or_else(|| GatewayError::InvalidPlan {
        detail: format!(
            "gateway {kind} path '{}' must be valid UTF-8",
            path.display()
        ),
    })
}
