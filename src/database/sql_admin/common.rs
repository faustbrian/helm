//! database sql admin common module.
//!
//! Contains database sql admin common logic used by Helm command workflows.

use anyhow::Result;

use crate::config::{Driver, ServiceConfig};

pub(super) struct SqlContext {
    pub(super) driver: Driver,
    pub(super) container_name: String,
    pub(super) db_name: String,
    pub(super) username: String,
    pub(super) password: String,
}

pub(super) fn sql_context(service: &ServiceConfig) -> Result<SqlContext> {
    let driver = sql_driver(service)?;

    Ok(SqlContext {
        driver,
        container_name: service.container_name()?,
        db_name: service.database.as_deref().unwrap_or("app").to_owned(),
        username: service.username.as_deref().unwrap_or("root").to_owned(),
        password: service.password.as_deref().unwrap_or("secret").to_owned(),
    })
}

pub(super) fn sql_driver(service: &ServiceConfig) -> Result<Driver> {
    match service.driver {
        Driver::Postgres | Driver::Mysql => Ok(service.driver),
        Driver::Redis
        | Driver::Valkey
        | Driver::Minio
        | Driver::Rustfs
        | Driver::Meilisearch
        | Driver::Typesense
        | Driver::Frankenphp
        | Driver::Gotenberg
        | Driver::Mailhog => anyhow::bail!("service '{}' is not SQL", service.name),
    }
}
