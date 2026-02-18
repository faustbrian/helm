//! cli args commands operations data module.
//!
//! Contains cli args commands operations data logic used by Helm command workflows.

use clap::Args;
use std::path::PathBuf;

use crate::cli::args::default_parallelism;
use crate::config;

#[derive(Args)]
pub(crate) struct RestoreArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long)]
    pub(crate) file: Option<PathBuf>,
    /// Drop and recreate the database before restoring
    #[arg(long, default_value_t = false)]
    pub(crate) reset: bool,
    /// Run `php artisan migrate` after restore
    #[arg(long, default_value_t = false)]
    pub(crate) migrate: bool,
    /// Run `php artisan schema:dump` after restore
    #[arg(long, default_value_t = false)]
    pub(crate) schema_dump: bool,
    /// Treat input as gzip-compressed SQL
    #[arg(long, default_value_t = false)]
    pub(crate) gzip: bool,
}

impl RestoreArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct DumpArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long)]
    pub(crate) file: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    pub(crate) stdout: bool,
    /// Write gzip-compressed SQL output
    #[arg(long, default_value_t = false)]
    pub(crate) gzip: bool,
}

impl DumpArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct PullArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long, default_value_t = default_parallelism())]
    pub(crate) parallel: usize,
}

impl PullArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }
}
