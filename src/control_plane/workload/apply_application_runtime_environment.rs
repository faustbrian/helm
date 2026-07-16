use crate::control_plane::ServiceExecutionPlan;
use std::collections::BTreeMap;

const LARAVEL_CONFIG_CACHE: &str = "/tmp/stackctl-laravel-config.php";

/// Applies runtime-owned environment required by the selected application preset.
pub(crate) fn apply_application_runtime_environment(
    application: &ServiceExecutionPlan,
    environment: &mut BTreeMap<String, String>,
) {
    if application.desired().preset() == Some("laravel") {
        environment.insert(
            "APP_CONFIG_CACHE".to_owned(),
            LARAVEL_CONFIG_CACHE.to_owned(),
        );
    }
}
