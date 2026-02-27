//! config types enums module.
//!
//! Contains config types enums logic used by Helm command workflows.

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Service category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Kind {
    /// SQL databases.
    Database,
    /// Cache backends.
    Cache,
    /// S3-compatible object storage.
    ObjectStore,
    /// Search engines.
    Search,
    /// Application runtimes (web apps, workers, tooling).
    App,
}

/// Service driver/backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Driver {
    /// `MongoDB`.
    Mongodb,
    /// `Memcached`.
    Memcached,
    /// `PostgreSQL`.
    Postgres,
    /// `MySQL`.
    Mysql,
    /// `SQL Server`.
    Sqlserver,
    /// `Redis`.
    Redis,
    /// `Valkey`.
    Valkey,
    /// `Dragonfly`.
    Dragonfly,
    /// `MinIO`.
    Minio,
    /// `Garage`.
    Garage,
    /// `RustFS`.
    Rustfs,
    /// `LocalStack`.
    Localstack,
    /// `Meilisearch`.
    Meilisearch,
    /// `Typesense`.
    Typesense,
    /// App runtime using `FrankenPHP`.
    Frankenphp,
    /// Laravel Reverb WebSocket server.
    Reverb,
    /// Laravel Horizon queue worker process.
    Horizon,
    /// Laravel scheduler worker process.
    Scheduler,
    /// Selenium standalone Chrome container for Laravel Dusk.
    Dusk,
    /// Gotenberg document conversion API.
    Gotenberg,
    /// `MailHog` local email testing server.
    Mailhog,
    /// `RabbitMQ` broker.
    Rabbitmq,
    /// `Soketi` WebSocket server.
    Soketi,
}

/// Container runtime engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ContainerEngine {
    /// Docker engine.
    Docker,
    /// Podman engine.
    Podman,
}

impl Default for ContainerEngine {
    fn default() -> Self {
        Self::Docker
    }
}

impl ContainerEngine {
    #[must_use]
    pub const fn command_binary(self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
        }
    }

    #[must_use]
    pub const fn host_gateway_alias(self) -> &'static str {
        match self {
            Self::Docker => "host.docker.internal",
            Self::Podman => "host.containers.internal",
        }
    }

    #[must_use]
    pub const fn host_gateway_mapping(self) -> Option<&'static str> {
        match self {
            Self::Docker => Some("host.docker.internal:host-gateway"),
            Self::Podman => None,
        }
    }
}
