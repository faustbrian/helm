//! Omnipresent local development control plane.
//!
//! Stackctl coordinates one per-user daemon and a Linux container workload
//! plane through the selected Docker Engine API.

#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::if_then_some_else_none,
        clippy::panic,
        clippy::str_to_string,
        clippy::unwrap_in_result,
        clippy::unwrap_used,
    )
)]

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
compile_error!("Stackctl v8 supports only macOS and Linux Unix hosts");

use anyhow::Result;
use clap::Parser;

mod cli;

use cli::args::Cli;
mod control_plane;
mod daemon;
mod javascript;
mod output;

fn main() -> Result<()> {
    let cli = Cli::parse();
    cli::dispatch::run(cli)
}
