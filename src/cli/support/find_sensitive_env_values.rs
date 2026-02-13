//! cli support find sensitive env values module.
//!
//! Contains cli support find sensitive env values logic used by Helm command workflows.

use anyhow::{Context, Result};
use std::path::Path;

pub(crate) fn find_sensitive_env_values(env_path: &Path) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(env_path)
        .with_context(|| format!("failed to read {}", env_path.display()))?;
    let sensitive_prefixes = [
        "APP_KEY",
        "DB_PASSWORD",
        "MAIL_PASSWORD",
        "AWS_SECRET_ACCESS_KEY",
        "MEILISEARCH_KEY",
        "TYPESENSE_API_KEY",
        "REDIS_PASSWORD",
    ];
    let mut matched = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, raw_value)) = trimmed.split_once('=') else {
            continue;
        };
        let value = raw_value.trim().trim_matches('"').trim_matches('\'');
        if sensitive_prefixes
            .iter()
            .any(|prefix| key.starts_with(prefix))
            && !value.is_empty()
            && value != "null"
            && !value.eq_ignore_ascii_case("changeme")
            && !value.eq_ignore_ascii_case("secret")
            && !value.eq_ignore_ascii_case("password")
        {
            matched.push(key.to_owned());
        }
    }
    Ok(matched)
}
