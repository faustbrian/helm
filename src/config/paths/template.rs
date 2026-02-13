//! config paths template module.
//!
//! Contains config paths template logic used by Helm command workflows.

pub(super) const DEFAULT_CONFIG_TEMPLATE: &str = r#"schema_version = 1
container_prefix = "my-app"

[[service]]
preset = "laravel"
name = "app"
domain = "my-app.localhost"
# depends_on = ["db", "redis"]

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
    "#;

#[cfg(test)]
mod tests {
    use super::DEFAULT_CONFIG_TEMPLATE;
    use crate::config::RawConfig;

    /// Returns the default value for template is valid toml and preset driven.
    #[test]
    fn default_template_is_valid_toml_and_preset_driven() {
        let raw: RawConfig = toml::from_str(DEFAULT_CONFIG_TEMPLATE).expect("parse template TOML");
        let presets: Vec<_> = raw
            .service
            .iter()
            .map(|service| service.preset.as_deref().unwrap_or(""))
            .collect();

        assert_eq!(presets, vec!["laravel", "mysql", "redis"]);
    }
}
