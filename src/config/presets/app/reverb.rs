//! Laravel Reverb app preset defaults.

use super::super::{Driver, Kind, PresetDefaults};

pub(super) fn defaults() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(Kind::App, Driver::Reverb, "dunglas/frankenphp:php8.5");
    defaults.name = Some("reverb");
    defaults.container_port = Some(8080);
    defaults.volumes = Some(vec![".:/app".to_owned()]);
    defaults.command = Some(vec![
        "php".to_owned(),
        "artisan".to_owned(),
        "reverb:start".to_owned(),
        "--host=0.0.0.0".to_owned(),
        "--port=8080".to_owned(),
    ]);
    defaults
}
