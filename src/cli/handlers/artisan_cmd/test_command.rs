//! cli handlers artisan cmd test command module.
//!
//! Contains cli handlers artisan cmd test command logic used by Helm command workflows.

use std::collections::HashMap;

/// Builds artisan test command for command execution.
pub(super) fn build_artisan_test_command(
    user_command: Vec<String>,
    inferred_env: &HashMap<String, String>,
) -> Vec<String> {
    let mut artisan_parts = vec!["php".to_owned(), "artisan".to_owned()];
    artisan_parts.extend(user_command);
    let escaped = artisan_parts
        .iter()
        .map(|part| shell_single_quote(part))
        .collect::<Vec<_>>()
        .join(" ");

    let mut exports: Vec<String> = inferred_env
        .iter()
        .filter_map(|(key, value)| {
            is_valid_env_key(key).then(|| format!("export {key}={}", shell_single_quote(value)))
        })
        .collect();
    exports.sort();
    exports.push("export APP_ENV='testing'".to_owned());

    let script = format!(
        "mkdir -p /tmp/helm-php/conf.d \\\n&& printf 'memory_limit=2048M\\n' > /tmp/helm-php/conf.d/zz-helm-memory.ini \\\n&& export PHP_INI_SCAN_DIR='/tmp/helm-php/conf.d:/usr/local/etc/php/conf.d' \\\n&& {} \\\n&& {escaped}",
        exports.join(" && ")
    );
    vec!["sh".to_owned(), "-lc".to_owned(), script]
}

fn shell_single_quote(value: &str) -> String {
    let escaped = value.replace('\'', "'\"'\"'");
    format!("'{escaped}'")
}

/// Returns whether the key matches the accepted environment-variable format.
fn is_valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(ch) if ch.is_ascii_alphabetic() || ch == '_' => {}
        _ => return false,
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}
