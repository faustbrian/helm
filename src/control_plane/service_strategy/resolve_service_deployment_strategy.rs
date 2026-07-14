use super::{ServiceDeploymentStrategy, ServiceStrategyError};

/// Resolves one preset into its conservative default workload scope.
pub(crate) fn resolve_service_deployment_strategy(
    preset: &str,
) -> Result<ServiceDeploymentStrategy, ServiceStrategyError> {
    let strategy = match preset {
        "mongodb" | "postgres" | "pg" | "pgsql" | "mysql" | "mariadb" | "sqlserver" | "mssql"
        | "redis" | "valkey" | "minio" | "rabbitmq" => {
            ServiceDeploymentStrategy::SharedByCompatibility
        }
        "mailpit" => ServiceDeploymentStrategy::SharedWithAttribution,
        "gotenberg" => ServiceDeploymentStrategy::SharedStateless,
        "memcached" | "localstack" => ServiceDeploymentStrategy::DedicatedProject,
        "dragonfly" | "garage" | "rustfs" | "opensearch" | "elasticsearch" | "meilisearch"
        | "typesense" => ServiceDeploymentStrategy::DedicatedUntilIsolationProven,
        "frankenphp" | "laravel" | "reverb" => ServiceDeploymentStrategy::ProjectApplication,
        "horizon" | "queue-worker" | "queue" => ServiceDeploymentStrategy::ProjectProcess,
        "scheduler" => ServiceDeploymentStrategy::ProjectScheduledCommand,
        "dusk" | "selenium" => ServiceDeploymentStrategy::Ephemeral,
        _ => return Err(ServiceStrategyError::unknown(preset)),
    };

    Ok(strategy)
}
