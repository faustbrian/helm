use super::{ServiceDeploymentStrategy, ServiceStrategyError};

/// Resolves one preset into its conservative default workload scope.
pub(crate) fn resolve_service_deployment_strategy(
    preset: &str,
) -> Result<ServiceDeploymentStrategy, ServiceStrategyError> {
    let strategy = match preset {
        "mongodb" | "postgres" | "pg" | "pgsql" | "mysql" | "mariadb" | "sqlserver" | "mssql"
        | "redis" | "valkey" | "minio" | "rustfs" | "rabbitmq" => {
            ServiceDeploymentStrategy::SharedByCompatibility
        }
        "mailhog" | "mailpit" => ServiceDeploymentStrategy::SharedWithAttribution,
        "gotenberg" => ServiceDeploymentStrategy::SharedStateless,
        "memcached" | "localstack" => ServiceDeploymentStrategy::DedicatedProject,
        "dragonfly" | "garage" | "opensearch" | "elasticsearch" | "meilisearch" | "typesense"
        | "soketi" => ServiceDeploymentStrategy::DedicatedUntilIsolationProven,
        "frankenphp" | "laravel" | "reverb" => ServiceDeploymentStrategy::ProjectApplication,
        "horizon" | "queue-worker" | "queue" | "scheduler" => {
            ServiceDeploymentStrategy::ProjectProcess
        }
        "dusk" | "selenium" => ServiceDeploymentStrategy::Ephemeral,
        _ => return Err(ServiceStrategyError::unknown(preset)),
    };

    Ok(strategy)
}
