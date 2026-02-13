//! config service methods ports module.
//!
//! Contains config service methods ports logic used by Helm command workflows.

use super::{Driver, ServiceConfig};

impl ServiceConfig {
    /// Returns the default internal service port for the driver.
    #[must_use]
    pub fn default_port(&self) -> u16 {
        let driver_port = match self.driver {
            Driver::Postgres => 5432,
            Driver::Mysql => 3306,
            Driver::Redis | Driver::Valkey => 6379,
            Driver::Minio => 9000,
            Driver::Rustfs => 9000,
            Driver::Meilisearch => 7700,
            Driver::Typesense => 8108,
            Driver::Frankenphp => 80,
            Driver::Gotenberg => 3000,
            Driver::Mailhog => 8025,
        };
        self.container_port.unwrap_or(driver_port)
    }

    /// Returns true when this is a SQL database service.
    #[must_use]
    pub const fn is_database(&self) -> bool {
        matches!(self.driver, Driver::Postgres | Driver::Mysql)
    }

    /// Returns true when this service supports dump/restore.
    #[must_use]
    pub const fn supports_sql_dump(&self) -> bool {
        self.is_database()
    }

    /// Laravel DB_CONNECTION-like driver name.
    #[must_use]
    pub const fn laravel_connection(&self) -> Option<&str> {
        match self.driver {
            Driver::Postgres => Some("pgsql"),
            Driver::Mysql => Some("mysql"),
            Driver::Redis | Driver::Valkey => Some("redis"),
            Driver::Minio
            | Driver::Rustfs
            | Driver::Meilisearch
            | Driver::Typesense
            | Driver::Frankenphp
            | Driver::Gotenberg
            | Driver::Mailhog => None,
        }
    }

    /// Returns internal container port.
    #[must_use]
    pub fn resolved_container_port(&self) -> u16 {
        self.default_port()
    }
}
