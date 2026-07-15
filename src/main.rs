//! Omnipresent local development control plane.
//!
//! Stackctl coordinates one per-user daemon and a Linux container workload
//! plane through the selected Docker Engine API.

#![allow(clippy::print_stdout)] // CLI tool needs to print to stdout
#![allow(clippy::clone_on_ref_ptr)] // Arc clones are explicit at call sites for clarity
#![allow(clippy::fn_params_excessive_bools)] // CLI options are represented directly as flags
#![allow(clippy::items_after_statements)] // Local helper functions keep related logic together
#![allow(dead_code, unused_imports)] // Internal control-plane contracts are selectively composed
#![allow(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(
    clippy::mod_module_files,
    clippy::wildcard_enum_match_arm,
    clippy::verbose_file_reads,
    clippy::unseparated_literal_suffix,
    clippy::unreachable,
    clippy::string_slice,
    clippy::shadow_unrelated,
    clippy::shadow_reuse,
    clippy::pattern_type_mismatch,
    clippy::indexing_slicing,
    clippy::empty_structs_with_brackets,
    clippy::as_conversions,
    clippy::print_stderr
)]
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
