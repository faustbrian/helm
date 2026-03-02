//! Laravel queue worker app preset defaults.

use super::super::{Driver, Kind, PresetDefaults};

pub(super) fn defaults() -> PresetDefaults {
    let mut defaults =
        PresetDefaults::base(Kind::App, Driver::Frankenphp, "dunglas/frankenphp:php8.5");
    defaults.name = Some("queue-worker");
    defaults.volumes = Some(vec![".:/app".to_owned()]);
    defaults.command = Some(vec![
        "php".to_owned(),
        "artisan".to_owned(),
        "queue:work".to_owned(),
        "--verbose".to_owned(),
        "--tries=3".to_owned(),
        "--timeout=90".to_owned(),
    ]);
    defaults.forced_env = Some(vec![("APP_ENV", "local")]);
    defaults
}
