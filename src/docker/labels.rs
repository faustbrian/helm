//! docker labels module.
//!
//! Contains Helm Docker label keys and values used by Helm command workflows.

use crate::config::Kind;

pub(crate) const LABEL_MANAGED: &str = "com.helm.managed";
pub(crate) const LABEL_SERVICE: &str = "com.helm.service";
pub(crate) const LABEL_KIND: &str = "com.helm.kind";
pub(crate) const LABEL_CONTAINER: &str = "com.helm.container";
pub(crate) const VALUE_MANAGED_TRUE: &str = "true";

pub(crate) fn kind_label_value(kind: Kind) -> &'static str {
    match kind {
        Kind::Database => "database",
        Kind::Cache => "cache",
        Kind::ObjectStore => "object-store",
        Kind::Search => "search",
        Kind::App => "app",
    }
}
