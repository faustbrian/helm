mod known_service_presets;
mod resolve_service_deployment_strategy;
mod service_deployment_strategy;
mod service_strategy_error;

pub(crate) use known_service_presets::KNOWN_SERVICE_PRESETS;
pub(crate) use resolve_service_deployment_strategy::resolve_service_deployment_strategy;
pub(crate) use service_deployment_strategy::ServiceDeploymentStrategy;
pub(crate) use service_strategy_error::ServiceStrategyError;
