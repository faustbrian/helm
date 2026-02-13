//! env mapping drivers module.
//!
//! Contains env mapping drivers logic used by Helm command workflows.

use std::collections::HashMap;

use crate::config::{Driver, ServiceConfig};

mod cache;
mod database;
mod object_store;
mod search;

pub(super) fn base_map_for_driver(service: &ServiceConfig) -> HashMap<String, String> {
    let mut map = HashMap::new();

    match service.driver {
        Driver::Mongodb | Driver::Postgres | Driver::Mysql => {
            database::apply_database_map(&mut map, service)
        }
        Driver::Redis | Driver::Valkey | Driver::Memcached => {
            cache::apply_cache_map(&mut map, service)
        }
        Driver::Minio | Driver::Rustfs => object_store::apply_object_store_map(&mut map, service),
        Driver::Meilisearch => search::apply_meilisearch_map(&mut map, service),
        Driver::Typesense => search::apply_typesense_map(&mut map, service),
        Driver::Frankenphp
        | Driver::Reverb
        | Driver::Horizon
        | Driver::Scheduler
        | Driver::Dusk
        | Driver::Gotenberg
        | Driver::Mailhog
        | Driver::Rabbitmq
        | Driver::Soketi => {}
    }

    map
}
