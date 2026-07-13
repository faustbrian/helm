use super::{
    CompatibilityFingerprintOptions, CompatibilityProfile, IsolationCapability, PersistenceMode,
    SharedDemandPlanError, SharedInstancePlan, SharedServiceRequest, plan_shared_instances,
};
use crate::control_plane::{ExecutionPlan, ServiceDeploymentStrategy};
use std::collections::BTreeMap;

/// Groups resolved shared-service demand by its exact compatibility identity.
pub(crate) fn resolve_execution_shared_instances(
    execution: &ExecutionPlan,
    platform: &str,
) -> Result<Vec<SharedInstancePlan>, SharedDemandPlanError> {
    let mut requests = Vec::new();

    for service in execution
        .services()
        .iter()
        .filter(|service| service.strategy() == ServiceDeploymentStrategy::SharedByCompatibility)
    {
        let preset = service.desired().preset().ok_or_else(|| {
            invalid(format!(
                "shared service '{}-{}' requires an explicit implementation preset",
                service.project().as_str(),
                service.service().as_str()
            ))
        })?;
        let implementation = match preset {
            "postgres" | "pg" | "pgsql" => "postgresql",
            "mysql" => "mysql",
            "mariadb" | "maria" => "mariadb",
            "redis" => "redis",
            "valkey" => "valkey",
            "minio" => "minio",
            "rabbitmq" => "rabbitmq",
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
        let profile = CompatibilityProfile::from_options(CompatibilityFingerprintOptions {
            implementation: implementation.to_owned(),
            major_version: version.to_owned(),
            image_digest: image.to_owned(),
            extensions: Vec::new(),
            immutable_settings: BTreeMap::new(),
            persistence: PersistenceMode::Persistent,
            isolation: match implementation {
                "redis" | "valkey" => IsolationCapability::AclAndPrefix,
                "minio" => IsolationCapability::BucketAndPolicy,
                "rabbitmq" => IsolationCapability::VirtualHostAndUser,
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

fn invalid(error: impl std::fmt::Display) -> SharedDemandPlanError {
    SharedDemandPlanError::new(error.to_string())
}
