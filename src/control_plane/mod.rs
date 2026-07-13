mod application;
mod configuration;
mod daemon;
mod desired_state;
mod dns_label;
mod engine;
mod environment_variable_key;
mod execution_plan;
mod gateway;
mod identity_error;
mod migration;
mod network;
mod project_identity;
mod registry_conflict;
mod retention;
mod route_claim;
mod route_identity;
mod service_identity;
mod service_strategy;
mod shared_infrastructure;
mod state;
mod tls;
mod validate_route_claims;
mod validated_route_registry;
mod workload;

pub(crate) use configuration::{parse_project_config, project_config_schema};
#[cfg(unix)]
pub(crate) use daemon::{
    IpcDiagnostic, IpcEvent, IpcEventKind, IpcManagedEnvironment, IpcNodePackageManager,
    IpcOutcome, IpcOutputStream, IpcPayload, IpcPhpTool, IpcProjectCommand, IpcProjectStatus,
    IpcRequest, IpcResourceLifecycle, IpcResourceStatus, IpcResponse, IpcResult,
    UnixDaemonWatchOptions, default_unix_daemon_runtime_directory, run_unix_daemon_watch,
    send_unix_request,
};
pub(crate) use desired_state::{
    DesiredProject, DesiredProjectError, DesiredService, resolve_desired_project,
};
use dns_label::DnsLabel;
pub(crate) use environment_variable_key::is_valid_environment_variable_key;
pub(crate) use execution_plan::{ExecutionPlan, ServiceExecutionPlan, resolve_execution_plan};
pub(crate) use identity_error::IdentityError;
pub(crate) use project_identity::ProjectIdentity;
pub(crate) use registry_conflict::{RegistryConflict, RegistryConflicts};
pub(crate) use route_claim::RouteClaim;
pub(crate) use route_identity::RouteIdentity;
pub(crate) use service_identity::ServiceIdentity;
pub(crate) use service_strategy::{
    KNOWN_SERVICE_PRESETS, ServiceDeploymentStrategy, ServiceStrategyError,
    resolve_service_deployment_strategy,
};
pub(crate) use shared_infrastructure::{
    CompatibilityFingerprint, CompatibilityFingerprintOptions, IsolationCapability, PersistenceMode,
};
#[cfg(target_os = "linux")]
pub(crate) use tls::DebianCertificateTrustStore;
#[cfg(target_os = "macos")]
pub(crate) use tls::MacOsCertificateTrustStore;
pub(crate) use tls::{
    CertificateTrustStore, CurrentCaTrustStatus, FilesystemCertificateStore,
    ProcessHostCommandExecutor, TrustChange, inspect_current_ca_trust, install_current_ca_trust,
    remove_current_ca_trust,
};
pub(crate) use validate_route_claims::validate_route_claims;
pub(crate) use validated_route_registry::ValidatedRouteRegistry;

#[cfg(test)]
mod tests;
