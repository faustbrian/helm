//! docker up args builder env module.
//!
//! Contains docker up args builder env logic used by Helm command workflows.

use crate::config::{Driver, ServiceConfig};

mod object_store;
mod search;
mod sql;

/// Appends run options to the caller-provided command or collection.
pub(super) fn append_run_options(args: &mut Vec<String>, service: &ServiceConfig) {
    append_volumes(args, service);
    append_custom_env(args, service);
    append_driver_env(args, service);
}

/// Appends volumes to the caller-provided command or collection.
fn append_volumes(args: &mut Vec<String>, service: &ServiceConfig) {
    if let Some(volumes) = &service.volumes {
        for volume in volumes {
            args.push("-v".to_owned());
            args.push(volume.clone());
        }
    }
}

/// Appends custom env to the caller-provided command or collection.
fn append_custom_env(args: &mut Vec<String>, service: &ServiceConfig) {
    if let Some(custom_env) = &service.env {
        for (key, value) in custom_env {
            args.push("-e".to_owned());
            args.push(format!("{key}={value}"));
        }
    }
}

/// Appends driver env to the caller-provided command or collection.
fn append_driver_env(args: &mut Vec<String>, service: &ServiceConfig) {
    match service.driver {
        Driver::Postgres | Driver::Mysql => sql::append(args, service),
        Driver::Meilisearch | Driver::Typesense => search::append(args, service),
        Driver::Minio | Driver::Rustfs => object_store::append(args, service),
        Driver::Redis
        | Driver::Valkey
        | Driver::Frankenphp
        | Driver::Gotenberg
        | Driver::Mailhog => {}
    }
}
