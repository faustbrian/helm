//! Unique testing runtime environment naming helpers.

use std::time::{SystemTime, UNIX_EPOCH};

/// Builds a unique runtime env namespace for one `helm artisan test` run.
pub(super) fn testing_runtime_env_name() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let run_id = format!("{:08x}", (nanos & 0xffff_ffff) as u32);

    format!("testing-{run_id}")
}

#[cfg(test)]
mod tests {
    use super::testing_runtime_env_name;

    #[test]
    fn runtime_env_name_uses_testing_prefix() {
        let runtime_env = testing_runtime_env_name();
        assert!(runtime_env.starts_with("testing-"));
    }

    #[test]
    fn runtime_env_name_has_non_empty_suffix() {
        let runtime_env = testing_runtime_env_name();
        let (_, suffix) = runtime_env
            .split_once('-')
            .expect("testing env should include separator");

        assert!(!suffix.is_empty());
    }
}
