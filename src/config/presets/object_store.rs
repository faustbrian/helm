//! config presets object store module.
//!
//! Contains config presets object store logic used by Helm command workflows.

use super::{Driver, Kind, PresetDefaults};

pub(super) const PRESET_NAMES: &[&str] = &["minio", "rustfs"];

/// Resolves resolve using configured inputs and runtime state.
pub(super) fn resolve(preset: &str) -> Option<PresetDefaults> {
    match preset {
        "minio" => Some(minio()),
        "rustfs" => Some(rustfs()),
        _ => None,
    }
}

fn minio() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(Kind::ObjectStore, Driver::Minio, "minio/minio:latest");
    defaults.name = Some("s3");
    defaults.bucket = Some("media");
    defaults.access_key = Some("minio");
    defaults.secret_key = Some("miniosecret");
    defaults.region = Some("us-east-1");
    defaults
}

fn rustfs() -> PresetDefaults {
    let mut defaults =
        PresetDefaults::base(Kind::ObjectStore, Driver::Rustfs, "rustfs/rustfs:latest");
    defaults.name = Some("s3");
    defaults.bucket = Some("media");
    defaults.access_key = Some("minio");
    defaults.secret_key = Some("miniosecret");
    defaults.region = Some("us-east-1");
    defaults
}
