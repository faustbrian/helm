//! Laravel Horizon app preset defaults.

use super::super::{Driver, Kind, PresetDefaults};
use super::extensions::laravel_extensions;

pub(super) fn defaults() -> PresetDefaults {
    let mut defaults =
        PresetDefaults::base(Kind::App, Driver::Horizon, "dunglas/frankenphp:php8.5");
    defaults.name = Some("horizon");
    defaults.php_extensions = Some(laravel_extensions());
    defaults.volumes = Some(vec![".:/app".to_owned()]);
    defaults.command = Some(vec![
        "php".to_owned(),
        "artisan".to_owned(),
        "horizon".to_owned(),
    ]);
    defaults.forced_env = Some(vec![("APP_ENV", "local"), ("QUEUE_CONNECTION", "redis")]);
    defaults
}
