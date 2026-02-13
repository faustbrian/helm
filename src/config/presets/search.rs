//! config presets search module.
//!
//! Contains config presets search logic used by Helm command workflows.

use super::{Driver, Kind, PresetDefaults};

pub(super) const PRESET_NAMES: &[&str] = &["meilisearch", "typesense"];

/// Resolves resolve using configured inputs and runtime state.
pub(super) fn resolve(preset: &str) -> Option<PresetDefaults> {
    match preset {
        "meilisearch" => Some(meilisearch()),
        "typesense" => Some(typesense()),
        _ => None,
    }
}

fn meilisearch() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(
        Kind::Search,
        Driver::Meilisearch,
        "getmeili/meilisearch:latest",
    );
    defaults.name = Some("search");
    defaults.api_key = Some("masterKey");
    defaults
}

fn typesense() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(
        Kind::Search,
        Driver::Typesense,
        "typesense/typesense:0.26.0",
    );
    defaults.name = Some("search");
    defaults.api_key = Some("xyz");
    defaults
}
