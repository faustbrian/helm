//! cli args commands operations docker ops inspect module.
//!
//! Contains inspect command args used by Helm command workflows.

use clap::Args;

use crate::config;

#[derive(Args)]
pub(crate) struct InspectArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long, conflicts_with = "json")]
    pub(crate) format: Option<String>,
    /// Emit structured JSON array output
    #[arg(long, default_value_t = false)]
    pub(crate) json: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) size: bool,
    #[arg(long = "type")]
    pub(crate) object_type: Option<String>,
}

impl InspectArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }

    pub(crate) fn format(&self) -> Option<&str> {
        self.format.as_deref()
    }

    pub(crate) fn object_type(&self) -> Option<&str> {
        self.object_type.as_deref()
    }
}
