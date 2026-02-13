//! Laravel scheduler app-target env inference.

use std::collections::HashMap;

use super::super::insert_if_absent;

/// Applies inferred Scheduler env keys.
pub(super) fn apply(vars: &mut HashMap<String, String>) {
    insert_if_absent(vars, "APP_ENV", "local".to_owned());
}
