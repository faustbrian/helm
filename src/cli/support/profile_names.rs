//! cli support profile names module.
//!
//! Contains cli support profile names logic used by Helm command workflows.

pub(crate) fn profile_names() -> Vec<&'static str> {
    vec!["full", "all", "infra", "data", "app", "web", "api"]
}
