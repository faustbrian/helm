//! Laravel scheduler app preset defaults.

use super::super::{Driver, Kind, PresetDefaults};

pub(super) fn defaults() -> PresetDefaults {
    let mut defaults =
        PresetDefaults::base(Kind::App, Driver::Scheduler, "dunglas/frankenphp:php8.5");
    defaults.name = Some("scheduler");
    defaults.volumes = Some(vec![".:/app".to_owned()]);
    defaults.command = Some(vec![
        "php".to_owned(),
        "artisan".to_owned(),
        "schedule:work".to_owned(),
    ]);
    defaults.forced_env = Some(vec![("APP_ENV", "local")]);
    defaults
}
