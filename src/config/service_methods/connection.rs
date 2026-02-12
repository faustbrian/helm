use super::{Driver, ServiceConfig};

impl ServiceConfig {
    /// Connection URL for clients.
    #[must_use]
    pub fn connection_url(&self) -> String {
        let scheme = self.scheme();
        let host = &self.host;
        let port = self.port;

        match self.driver {
            Driver::Postgres | Driver::Mysql => {
                let user = self.username.as_deref().unwrap_or("root");
                let db = self.database.as_deref().unwrap_or("app");
                let password = self.password.as_deref().unwrap_or("");
                let db_scheme = if matches!(self.driver, Driver::Postgres) {
                    "postgresql"
                } else {
                    "mysql"
                };
                if password.is_empty() {
                    format!("{db_scheme}://{user}@{host}:{port}/{db}")
                } else {
                    format!("{db_scheme}://{user}:{password}@{host}:{port}/{db}")
                }
            }
            Driver::Redis | Driver::Valkey => {
                let user = self.username.as_deref().unwrap_or("");
                let password = self.password.as_deref().unwrap_or("");
                if user.is_empty() && password.is_empty() {
                    format!("redis://{host}:{port}")
                } else if user.is_empty() {
                    format!("redis://:{password}@{host}:{port}")
                } else {
                    format!("redis://{user}:{password}@{host}:{port}")
                }
            }
            Driver::Minio | Driver::Rustfs => format!("{scheme}://{host}:{port}"),
            Driver::Meilisearch => format!("{scheme}://{host}:{port}"),
            Driver::Typesense => format!("{scheme}://{host}:{port}"),
            Driver::Frankenphp | Driver::Gotenberg | Driver::Mailhog => {
                format!("{scheme}://{host}:{port}")
            }
        }
    }
}
