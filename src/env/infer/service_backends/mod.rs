//! Env inference dispatcher for backend service drivers.

use std::collections::HashMap;

use crate::config::{Driver, ServiceConfig};

mod cache;
mod database;
mod object_store;
mod search;

/// Applies inferred backend env variables for a single non-app service.
pub(super) fn apply_service_env(vars: &mut HashMap<String, String>, service: &ServiceConfig) {
    match service.driver {
        Driver::Postgres | Driver::Mysql => database::apply(vars, service),
        Driver::Redis | Driver::Valkey => cache::apply(vars, service),
        Driver::Minio | Driver::Rustfs => object_store::apply(vars, service),
        Driver::Meilisearch | Driver::Typesense => search::apply(vars, service),
        Driver::Frankenphp | Driver::Gotenberg | Driver::Mailhog => {}
    }
}
