//! Common docker run env and volume argument helpers.

use crate::config::{Driver, ServiceConfig};

/// Appends volumes to the caller-provided command or collection.
pub(super) fn append_volumes(
    args: &mut Vec<String>,
    service: &ServiceConfig,
    container_name: &str,
) {
    if let Some(volumes) = &service.volumes {
        for volume in volumes {
            args.push("-v".to_owned());
            args.push(volume.clone());
        }
        return;
    }

    if let Some(data_dir) = default_persistent_data_dir(service) {
        args.push("-v".to_owned());
        args.push(format!("{container_name}-data:{data_dir}"));
    }
}

/// Appends custom env to the caller-provided command or collection.
pub(super) fn append_custom_env(args: &mut Vec<String>, service: &ServiceConfig) {
    if let Some(custom_env) = &service.env {
        for (key, value) in custom_env {
            args.push("-e".to_owned());
            args.push(format!("{key}={value}"));
        }
    }
}

fn default_persistent_data_dir(service: &ServiceConfig) -> Option<&'static str> {
    match service.driver {
        Driver::Mongodb => Some("/data/db"),
        Driver::Postgres => Some("/var/lib/postgresql/data"),
        Driver::Mysql => Some("/var/lib/mysql"),
        Driver::Sqlserver => Some("/var/opt/mssql"),
        Driver::Redis | Driver::Valkey | Driver::Dragonfly => Some("/data"),
        Driver::Minio | Driver::Rustfs => Some("/data"),
        Driver::Garage => Some("/var/lib/garage"),
        Driver::Localstack => Some("/var/lib/localstack"),
        Driver::Meilisearch => Some("/meili_data"),
        Driver::Typesense => Some("/data"),
        Driver::Memcached
        | Driver::Frankenphp
        | Driver::Reverb
        | Driver::Horizon
        | Driver::Scheduler
        | Driver::Dusk
        | Driver::Gotenberg
        | Driver::Mailhog
        | Driver::Rabbitmq
        | Driver::Soketi => None,
    }
}
