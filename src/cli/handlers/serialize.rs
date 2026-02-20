//! Shared serialization/rendering helpers for CLI handlers.

use anyhow::{Result, bail};
use serde::Serialize;

pub(crate) fn print_json_pretty<T: Serialize + ?Sized>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

pub(crate) fn print_toml_pretty<T: Serialize + ?Sized>(value: &T) -> Result<()> {
    println!("{}", toml::to_string_pretty(value)?);
    Ok(())
}

pub(crate) fn print_pretty<T: Serialize + ?Sized>(value: &T, format: &str) -> Result<()> {
    match format {
        "json" => print_json_pretty(value),
        "toml" => print_toml_pretty(value),
        _ => bail!("unsupported format: {format}"),
    }
}
