pub(super) fn default_health_path_for_preset(preset: &str) -> Option<&'static str> {
    match preset {
        "laravel" | "laravel-full" | "laravel-minimal" => Some("/up"),
        "gotenberg" => Some("/health"),
        "mailhog" => Some("/"),
        _ => None,
    }
}

pub(super) fn default_health_statuses_for_preset(preset: &str) -> Option<Vec<u16>> {
    match preset {
        "laravel" | "laravel-full" | "laravel-minimal" | "gotenberg" | "mailhog" => Some(vec![200]),
        _ => None,
    }
}
