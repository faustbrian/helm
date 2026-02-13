//! docker exec args module.
//!
//! Contains docker exec args logic used by Helm command workflows.

use anyhow::Result;

use crate::config::{Driver, ServiceConfig};

/// Builds exec args for command execution.
pub(super) fn build_exec_args(container_name: &str, command: &[String], tty: bool) -> Vec<String> {
    let mut args = vec![
        "exec".to_owned(),
        if tty { "-it" } else { "-i" }.to_owned(),
        container_name.to_owned(),
    ];
    args.extend(command.iter().cloned());
    args
}

pub(super) fn interactive_client_args(service: &ServiceConfig) -> Vec<String> {
    match service.driver {
        Driver::Postgres => vec![
            "psql".to_owned(),
            "-U".to_owned(),
            service
                .username
                .clone()
                .unwrap_or_else(|| "postgres".to_owned()),
            service.database.clone().unwrap_or_else(|| "app".to_owned()),
        ],
        Driver::Mysql => vec![
            "mysql".to_owned(),
            "-u".to_owned(),
            service
                .username
                .clone()
                .unwrap_or_else(|| "root".to_owned()),
            format!("-p{}", service.password.as_deref().unwrap_or("secret")),
            service.database.clone().unwrap_or_else(|| "app".to_owned()),
        ],
        Driver::Redis | Driver::Valkey => vec![
            "redis-cli".to_owned(),
            "-h".to_owned(),
            "127.0.0.1".to_owned(),
            "-p".to_owned(),
            service.default_port().to_string(),
        ],
        Driver::Minio
        | Driver::Rustfs
        | Driver::Meilisearch
        | Driver::Typesense
        | Driver::Frankenphp
        | Driver::Gotenberg
        | Driver::Mailhog => {
            vec!["sh".to_owned()]
        }
    }
}

pub(super) fn piped_client_args(service: &ServiceConfig) -> Result<Vec<String>> {
    match service.driver {
        Driver::Postgres => Ok(vec![
            "psql".to_owned(),
            "-U".to_owned(),
            service
                .username
                .clone()
                .unwrap_or_else(|| "postgres".to_owned()),
            service.database.clone().unwrap_or_else(|| "app".to_owned()),
        ]),
        Driver::Mysql => Ok(vec![
            "mysql".to_owned(),
            "-u".to_owned(),
            service
                .username
                .clone()
                .unwrap_or_else(|| "root".to_owned()),
            format!("-p{}", service.password.as_deref().unwrap_or("secret")),
            service.database.clone().unwrap_or_else(|| "app".to_owned()),
        ]),
        Driver::Redis
        | Driver::Valkey
        | Driver::Minio
        | Driver::Rustfs
        | Driver::Meilisearch
        | Driver::Typesense
        | Driver::Frankenphp
        | Driver::Gotenberg
        | Driver::Mailhog => {
            anyhow::bail!("piped exec is supported for SQL services only")
        }
    }
}
