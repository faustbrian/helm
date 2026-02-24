//! config paths template module.
//!
//! Contains config paths template logic used by Helm command workflows.

/// Builds default `.helm.toml` template content for the given project name.
#[must_use]
pub(super) fn default_config_template(project_name: &str) -> String {
    let slug = sanitize_project_slug(project_name);
    let domain = format!("{slug}.localhost");
    format!(
        r#"schema_version = 1
container_engine = "docker"
container_prefix = "{slug}"

[[service]]
preset = "laravel"
name = "app"
domain = "{domain}"
# depends_on = ["db", "redis"]
#
# Optional lifecycle hooks:
# [[service.hook]]
# name = "seed-dev-user"
# phase = "post_up"
# on_error = "fail"
# [service.hook.run]
# type = "exec"
# argv = ["php", "artisan", "db:seed", "--class=DevUserSeeder"]

[[service]]
preset = "mysql"
name = "db"

[[service]]
preset = "redis"

# Optional services:
#[[service]]
# preset = "minio"
# name = "s3"

#[[service]]
# preset = "meilisearch"
# name = "search"

#[[service]]
# preset = "mailhog"
# name = "mailhog"

#[[service]]
# preset = "dusk"
# name = "dusk"
# depends_on = ["app"]
    "#
    )
}

fn sanitize_project_slug(name: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;

    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            previous_dash = false;
            continue;
        }

        if !previous_dash {
            slug.push('-');
            previous_dash = true;
        }
    }

    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "my-app".to_owned()
    } else {
        slug.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::default_config_template;
    use crate::config::RawConfig;

    /// Returns the default value for template is valid toml and preset driven.
    #[test]
    fn default_template_is_valid_toml_and_preset_driven() {
        let raw: RawConfig =
            toml::from_str(&default_config_template("my app")).expect("parse template TOML");
        let presets: Vec<_> = raw
            .service
            .iter()
            .map(|service| service.preset.as_deref().unwrap_or(""))
            .collect();

        assert_eq!(presets, vec!["laravel", "mysql", "redis"]);
    }

    #[test]
    fn default_template_uses_sanitized_project_slug_for_prefix_and_domain() {
        let raw: RawConfig =
            toml::from_str(&default_config_template("Billing API")).expect("parse template TOML");

        assert_eq!(raw.container_prefix.as_deref(), Some("billing-api"));

        let app = raw
            .service
            .iter()
            .find(|service| service.name.as_deref() == Some("app"))
            .expect("app service present");
        assert_eq!(app.domain.as_deref(), Some("billing-api.localhost"));
    }

    #[test]
    fn default_template_falls_back_when_project_name_has_no_ascii_alnum() {
        let raw: RawConfig =
            toml::from_str(&default_config_template("___")).expect("parse template TOML");

        assert_eq!(raw.container_prefix.as_deref(), Some("my-app"));
    }
}
