use super::{Driver, Kind, PresetDefaults};

pub(super) const PRESET_NAMES: &[&str] = &["redis", "valkey"];

pub(super) fn resolve(preset: &str) -> Option<PresetDefaults> {
    match preset {
        "redis" => Some(redis()),
        "valkey" => Some(valkey()),
        _ => None,
    }
}

fn redis() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(Kind::Cache, Driver::Redis, "redis:7-alpine");
    defaults.name = Some("redis");
    defaults
}

fn valkey() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(Kind::Cache, Driver::Valkey, "valkey/valkey:8");
    defaults.name = Some("valkey");
    defaults
}
