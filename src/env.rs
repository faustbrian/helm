//! .env file updater for service variables.

#![allow(clippy::print_stdout)] // Env updates need to print status

mod files;
mod infer;
mod mapping;
#[cfg(test)]
mod tests;

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::config::{Config, ServiceConfig};

pub fn update_env(service: &ServiceConfig, env_path: &Path, create_missing: bool) -> Result<()> {
    files::update_env(service, env_path, create_missing)
}

#[must_use]
pub fn inferred_app_env(config: &Config) -> HashMap<String, String> {
    infer::inferred_app_env(config)
}

pub fn write_env_values(
    env_path: &Path,
    values: &HashMap<String, String>,
    create_missing: bool,
) -> Result<()> {
    files::write_env_values(env_path, values, create_missing)
}

pub fn write_env_values_with_purge(
    env_path: &Path,
    values: &HashMap<String, String>,
    create_missing: bool,
    managed_keys: &HashSet<String>,
    purge_missing_managed: bool,
) -> Result<()> {
    files::write_env_values_with_purge(
        env_path,
        values,
        create_missing,
        managed_keys,
        purge_missing_managed,
    )
}

pub fn write_env_values_full(env_path: &Path, values: &HashMap<String, String>) -> Result<()> {
    files::write_env_values_full(env_path, values)
}

#[must_use]
pub fn managed_app_env(config: &Config) -> HashMap<String, String> {
    infer::managed_app_env(config)
}
