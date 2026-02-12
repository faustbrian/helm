use serde_json::Value;

use crate::config::ServiceConfig;

pub(super) fn body_health_is_ok(target: &ServiceConfig, body: &str) -> bool {
    if target.driver != crate::config::Driver::Gotenberg {
        return true;
    }

    let parsed: Value = match serde_json::from_str(body) {
        Ok(value) => value,
        Err(_) => return false,
    };
    parsed
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| status.eq_ignore_ascii_case("up"))
}
