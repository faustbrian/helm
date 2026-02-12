use super::super::{Driver, Kind, PresetDefaults};

use super::extensions::{laravel_extensions, laravel_minimal_extensions};

pub(super) fn frankenphp() -> PresetDefaults {
    let mut defaults =
        PresetDefaults::base(Kind::App, Driver::Frankenphp, "dunglas/frankenphp:php8.5");
    defaults.name = Some("app");
    defaults.container_port = Some(80);
    defaults.php_extensions = Some(laravel_minimal_extensions());
    defaults.volumes = Some(vec![".:/app".to_owned()]);
    defaults.trust_container_ca = true;
    defaults
}

pub(super) fn laravel() -> PresetDefaults {
    let mut defaults =
        PresetDefaults::base(Kind::App, Driver::Frankenphp, "dunglas/frankenphp:php8.5");
    defaults.name = Some("app");
    defaults.container_port = Some(80);
    defaults.php_extensions = Some(laravel_extensions());
    defaults.volumes = Some(vec![".:/app".to_owned()]);
    defaults.forced_env = Some(vec![("APP_ENV", "local")]);
    defaults.trust_container_ca = true;
    defaults
}

pub(super) fn laravel_minimal() -> PresetDefaults {
    let mut defaults =
        PresetDefaults::base(Kind::App, Driver::Frankenphp, "dunglas/frankenphp:php8.5");
    defaults.name = Some("app");
    defaults.container_port = Some(80);
    defaults.php_extensions = Some(laravel_minimal_extensions());
    defaults.volumes = Some(vec![".:/app".to_owned()]);
    defaults.trust_container_ca = true;
    defaults
}

pub(super) fn gotenberg() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(Kind::App, Driver::Gotenberg, "gotenberg/gotenberg:8");
    defaults.name = Some("gotenberg");
    defaults.container_port = Some(3000);
    defaults
}

pub(super) fn mailhog() -> PresetDefaults {
    let mut defaults = PresetDefaults::base(Kind::App, Driver::Mailhog, "mailhog/mailhog:latest");
    defaults.name = Some("mailhog");
    defaults.container_port = Some(8025);
    defaults
}
