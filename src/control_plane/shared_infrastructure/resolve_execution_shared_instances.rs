use super::{
    CompatibilityFingerprintOptions, CompatibilityProfile, IsolationCapability, PersistenceMode,
    SharedDemandPlanError, SharedInstancePlan, SharedServiceRequest, plan_shared_instances,
};
use crate::control_plane::{ExecutionPlan, ServiceDeploymentStrategy, ServiceExecutionPlan};
use std::collections::BTreeMap;

/// Groups resolved shared-service demand by its exact compatibility identity.
pub(crate) fn resolve_execution_shared_instances(
    execution: &ExecutionPlan,
    platform: &str,
) -> Result<Vec<SharedInstancePlan>, SharedDemandPlanError> {
    let mut requests = Vec::new();

    for service in execution.services().iter().filter(|service| {
        matches!(
            service.strategy(),
            ServiceDeploymentStrategy::SharedByCompatibility
                | ServiceDeploymentStrategy::SharedWithAttribution
        )
    }) {
        let preset = service.desired().preset().ok_or_else(|| {
            invalid(format!(
                "shared service '{}-{}' requires an explicit implementation preset",
                service.project().as_str(),
                service.service().as_str()
            ))
        })?;
        let implementation = match preset {
            "postgres" | "pg" | "pgsql" => "postgresql",
            "mongodb" => "mongodb",
            "sqlserver" | "mssql" => "sqlserver",
            "mysql" => "mysql",
            "mariadb" | "maria" => "mariadb",
            "redis" => "redis",
            "valkey" => "valkey",
            "minio" => "minio",
            "rabbitmq" => "rabbitmq",
            "mailpit" => "mailpit",
            _ => {
                return Err(invalid(format!(
                    "shared service '{}-{}' preset '{preset}' has no compatibility profile resolver",
                    service.project().as_str(),
                    service.service().as_str()
                )));
            }
        };
        let version = service.desired().version().ok_or_else(|| {
            invalid(format!(
                "shared service '{}-{}' requires an exact major version",
                service.project().as_str(),
                service.service().as_str()
            ))
        })?;
        if version.is_empty() || !version.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(invalid(format!(
                "shared service '{}-{}' major version '{version}' must be numeric",
                service.project().as_str(),
                service.service().as_str()
            )));
        }
        let image = service.desired().image().ok_or_else(|| {
            invalid(format!(
                "shared service '{}-{}' requires a resolved immutable image artifact",
                service.project().as_str(),
                service.service().as_str()
            ))
        })?;
        let immutable_settings = immutable_settings(service, implementation)?;
        let profile = CompatibilityProfile::from_options(CompatibilityFingerprintOptions {
            implementation: implementation.to_owned(),
            major_version: version.to_owned(),
            image_digest: image.to_owned(),
            extensions: Vec::new(),
            immutable_settings,
            persistence: PersistenceMode::Persistent,
            isolation: match implementation {
                "redis" | "valkey" => IsolationCapability::AclAndPrefix,
                "minio" => IsolationCapability::BucketAndPolicy,
                "rabbitmq" => IsolationCapability::VirtualHostAndUser,
                "mailpit" => IsolationCapability::None,
                _ => IsolationCapability::DatabaseAndRole,
            },
            platform_architecture: Some(platform.to_owned()),
        })
        .map_err(invalid)?;
        requests.push(SharedServiceRequest::new(
            service.project().as_str(),
            service.service().as_str(),
            profile,
        ));
    }

    Ok(plan_shared_instances(requests))
}

fn immutable_settings(
    service: &ServiceExecutionPlan,
    implementation: &str,
) -> Result<BTreeMap<String, String>, SharedDemandPlanError> {
    if implementation != "sqlserver" {
        return Ok(BTreeMap::new());
    }
    let environment = service.desired().environment();
    if environment.get("ACCEPT_EULA").map(String::as_str) != Some("Y") {
        return Err(invalid(format!(
            "shared SQL Server service '{}-{}' requires environment.ACCEPT_EULA: \"Y\"",
            service.project().as_str(),
            service.service().as_str()
        )));
    }
    if let Some(key) = environment
        .keys()
        .find(|key| !matches!(key.as_str(), "ACCEPT_EULA" | "MSSQL_PID"))
    {
        return Err(invalid(format!(
            "shared SQL Server service '{}-{}' declares unsupported environment key '{key}'",
            service.project().as_str(),
            service.service().as_str()
        )));
    }

    Ok(BTreeMap::from([(
        "edition".to_owned(),
        environment
            .get("MSSQL_PID")
            .cloned()
            .unwrap_or_else(|| "Developer".to_owned()),
    )]))
}

fn invalid(error: impl std::fmt::Display) -> SharedDemandPlanError {
    SharedDemandPlanError::new(error.to_string())
}
