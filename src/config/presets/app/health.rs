//! config presets app health module.
//!
//! Contains config presets app health logic used by Helm command workflows.

/// Returns the default value for health path for preset.
pub(super) fn default_health_path_for_preset(preset: &str) -> Option<&'static str> {
    match preset {
        "laravel" | "laravel-full" | "laravel-minimal" => Some("/up"),
        "gotenberg" => Some("/health"),
        "mailhog" => Some("/"),
        _ => None,
    }
}

/// Returns the default value for health statuses for preset.
pub(super) fn default_health_statuses_for_preset(preset: &str) -> Option<Vec<u16>> {
    match preset {
        "laravel" | "laravel-full" | "laravel-minimal" | "gotenberg" | "mailhog" => Some(vec![200]),
        _ => None,
    }
}
