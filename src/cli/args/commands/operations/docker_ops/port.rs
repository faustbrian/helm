//! cli args commands operations docker ops port module.
//!
//! Contains port command args used by Helm command workflows.

use clap::Args;

use crate::config;

#[derive(Args)]
pub(crate) struct PortArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Emit structured JSON array output
    #[arg(long, default_value_t = false)]
    pub(crate) json: bool,
    /// Optional private port/protocol, for example `80/tcp`
    pub(crate) private_port: Option<String>,
}

impl PortArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }

    pub(crate) fn private_port(&self) -> Option<&str> {
        self.private_port.as_deref()
    }
}
