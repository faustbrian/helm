//! config types service module.
//!
//! Contains config types service logic used by Helm command workflows.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{Driver, Kind, ServiceHook};

/// Configuration for a single service instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ServiceConfig {
    /// Unique service name.
    pub name: String,
    /// Service kind.
    pub kind: Kind,
    /// Backend driver.
    pub driver: Driver,
    /// Docker image.
    pub image: String,
    /// Host bind address.
    pub host: String,
    /// Host port.
    pub port: u16,
    /// Database name for SQL services.
    pub database: Option<String>,
    /// Username for services with auth.
    pub username: Option<String>,
    /// Password for services with auth.
    pub password: Option<String>,
    /// Object store bucket.
    pub bucket: Option<String>,
    /// Access key for object store.
    pub access_key: Option<String>,
    /// Secret key for object store.
    pub secret_key: Option<String>,
    /// API key for search services.
    pub api_key: Option<String>,
    /// Region for object store.
    pub region: Option<String>,
    /// URL scheme override (`http`, `https`).
    pub scheme: Option<String>,
    /// Public domain for app services.
    #[serde(default)]
    pub domain: Option<String>,
    /// Additional public domains for app services.
    #[serde(default)]
    pub domains: Option<Vec<String>>,
    /// Optional internal container port override.
    #[serde(default)]
    pub container_port: Option<u16>,
    /// Optional host SMTP port for mail testing services.
    #[serde(default)]
    pub smtp_port: Option<u16>,
    /// Optional Docker volume mounts.
    #[serde(default)]
    pub volumes: Option<Vec<String>>,
    /// Optional Docker environment variables passed to container.
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
    /// Optional command and args for the container entrypoint.
    #[serde(default)]
    pub command: Option<Vec<String>>,
    /// Optional service dependencies that should be started first.
    #[serde(default)]
    pub depends_on: Option<Vec<String>>,
    /// Optional SQL seed file applied when running `up --with-data`.
    #[serde(default)]
    pub seed_file: Option<String>,
    /// Lifecycle hooks executed during `up`/`down` operations.
    #[serde(default)]
    pub hook: Vec<ServiceHook>,
    /// Optional app health check path.
    #[serde(default)]
    pub health_path: Option<String>,
    /// Optional accepted health status codes.
    #[serde(default)]
    pub health_statuses: Option<Vec<u16>>,
    /// Serve app directly via <https://localhost>:<port> without host Caddy routing.
    #[serde(default)]
    pub localhost_tls: bool,
    /// Run this app with Laravel Octane (frankenphp) when no custom command is set.
    #[serde(default)]
    pub octane: bool,
    /// Optional PHP extensions to auto-install into a derived serve image.
    #[serde(default)]
    pub php_extensions: Option<Vec<String>>,
    /// Trust inner container Caddy CA in local system trust store.
    #[serde(default)]
    pub trust_container_ca: bool,
    /// Optional Laravel env var name overrides.
    #[serde(default)]
    pub env_mapping: Option<HashMap<String, String>>,
    /// Explicit docker container name for this service.
    #[serde(default)]
    pub container_name: Option<String>,
    /// Resolved container name at runtime (not serialized).
    #[serde(skip)]
    pub resolved_container_name: Option<String>,
}
