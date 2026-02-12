use std::collections::HashMap;

use crate::config::ServiceConfig;

use super::super::{insert_if_absent, runtime_host_for_app};

pub(super) fn apply(vars: &mut HashMap<String, String>, service: &ServiceConfig) {
    insert_if_absent(vars, "CACHE_STORE", "redis".to_owned());
    insert_if_absent(vars, "QUEUE_CONNECTION", "redis".to_owned());
    insert_if_absent(vars, "SESSION_DRIVER", "redis".to_owned());
    insert_if_absent(vars, "SESSION_CONNECTION", "default".to_owned());
    insert_if_absent(vars, "REDIS_CLIENT", "phpredis".to_owned());
    let redis_host = runtime_host_for_app(service);
    let redis_port = service.port.to_string();
    let redis_username = service.username.clone().unwrap_or_default();
    let redis_password = service.password.clone().unwrap_or_default();

    insert_if_absent(vars, "REDIS_HOST", redis_host.clone());
    insert_if_absent(vars, "REDIS_PORT", redis_port.clone());
    insert_if_absent(vars, "REDIS_USERNAME", redis_username.clone());
    insert_if_absent(vars, "REDIS_PASSWORD", redis_password.clone());
    insert_if_absent(vars, "REDIS_CACHE_HOST", redis_host);
    insert_if_absent(vars, "REDIS_CACHE_PORT", redis_port);
    insert_if_absent(vars, "REDIS_CACHE_USERNAME", redis_username);
    insert_if_absent(vars, "REDIS_CACHE_PASSWORD", redis_password);
}
