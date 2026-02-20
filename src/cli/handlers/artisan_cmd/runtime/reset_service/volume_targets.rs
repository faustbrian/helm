//! Named volume target discovery for runtime reset.

use anyhow::Result;
use std::collections::BTreeSet;
use std::path::Path;

use crate::config::{Driver, ServiceConfig};

pub(super) fn collect_named_volume_targets(service: &ServiceConfig) -> Result<Vec<String>> {
    let mut named_volumes = BTreeSet::new();

    if service.volumes.is_none() && uses_default_named_data_volume(service) {
        named_volumes.insert(format!("{}-data", service.container_name()?));
    }

    if let Some(volumes) = &service.volumes {
        for volume in volumes {
            if let Some(named_volume) = extract_named_volume_source(volume) {
                named_volumes.insert(named_volume);
            }
        }
    }

    Ok(named_volumes.into_iter().collect())
}

fn extract_named_volume_source(volume: &str) -> Option<String> {
    let mut parts = volume.splitn(3, ':');
    let source = parts.next()?.trim();
    let _target = parts.next()?;

    if source.is_empty() || is_bind_mount_source(source) {
        return None;
    }

    Some(source.to_owned())
}

fn is_bind_mount_source(source: &str) -> bool {
    source.starts_with('.')
        || source.starts_with('~')
        || source.contains('/')
        || source.contains('\\')
        || Path::new(source).is_absolute()
}

fn uses_default_named_data_volume(service: &ServiceConfig) -> bool {
    matches!(
        service.driver,
        Driver::Mongodb
            | Driver::Postgres
            | Driver::Mysql
            | Driver::Sqlserver
            | Driver::Redis
            | Driver::Valkey
            | Driver::Dragonfly
            | Driver::Minio
            | Driver::Rustfs
            | Driver::Localstack
            | Driver::Meilisearch
            | Driver::Typesense
    )
}
