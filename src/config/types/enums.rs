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
    /// `PostgreSQL`.
    Postgres,
    /// `MySQL`.
    Mysql,
    /// `Redis`.
    Redis,
    /// `Valkey`.
    Valkey,
    /// `MinIO`.
    Minio,
    /// `RustFS`.
    Rustfs,
    /// `Meilisearch`.
    Meilisearch,
    /// `Typesense`.
    Typesense,
    /// App runtime using `FrankenPHP`.
    Frankenphp,
    /// Gotenberg document conversion API.
    Gotenberg,
    /// `MailHog` local email testing server.
    Mailhog,
}
