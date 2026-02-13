//! config presets app module.
//!
//! Contains config presets app logic used by Helm command workflows.

use super::PresetDefaults;

mod defaults;
mod extensions;
mod health;

pub(super) const PRESET_NAMES: &[&str] = &[
    "frankenphp",
    "laravel",
    "laravel-minimal",
    "laravel-full",
    "gotenberg",
    "mailhog",
];

/// Resolves resolve using configured inputs and runtime state.
pub(super) fn resolve(preset: &str) -> Option<PresetDefaults> {
    match preset {
        "frankenphp" => Some(defaults::frankenphp()),
        "laravel" | "laravel-full" => Some(defaults::laravel()),
        "laravel-minimal" => Some(defaults::laravel_minimal()),
        "gotenberg" => Some(defaults::gotenberg()),
        "mailhog" => Some(defaults::mailhog()),
        _ => None,
    }
}

/// Returns the default value for health path for preset.
pub(super) fn default_health_path_for_preset(preset: &str) -> Option<&'static str> {
    health::default_health_path_for_preset(preset)
}

/// Returns the default value for health statuses for preset.
pub(super) fn default_health_statuses_for_preset(preset: &str) -> Option<Vec<u16>> {
    health::default_health_statuses_for_preset(preset)
}
