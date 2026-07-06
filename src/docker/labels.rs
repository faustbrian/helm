//! docker labels module.
//!
//! Contains Stackctl Docker label keys and values used by Stackctl command workflows.

use crate::config::Kind;

pub(crate) const LABEL_MANAGED: &str = "com.stackctl.managed";
pub(crate) const LABEL_SERVICE: &str = "com.stackctl.service";
pub(crate) const LABEL_KIND: &str = "com.stackctl.kind";
pub(crate) const LABEL_CONTAINER: &str = "com.stackctl.container";
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
