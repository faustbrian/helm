use std::collections::HashMap;

use crate::config::ServiceConfig;

pub(super) fn apply_cache_map(map: &mut HashMap<String, String>, service: &ServiceConfig) {
    map.insert("CACHE_STORE".to_owned(), "redis".to_owned());
    map.insert("QUEUE_CONNECTION".to_owned(), "redis".to_owned());
    map.insert("SESSION_DRIVER".to_owned(), "redis".to_owned());
    map.insert("REDIS_CLIENT".to_owned(), "phpredis".to_owned());
    let redis_host = service.host.clone();
    let redis_port = service.port.to_string();
    let redis_username = service.username.clone().unwrap_or_default();
    let redis_password = service.password.clone().unwrap_or_default();

    map.insert("REDIS_HOST".to_owned(), redis_host.clone());
    map.insert("REDIS_PORT".to_owned(), redis_port.clone());
    map.insert("REDIS_USERNAME".to_owned(), redis_username.clone());
    map.insert("REDIS_PASSWORD".to_owned(), redis_password.clone());
    map.insert("REDIS_CACHE_HOST".to_owned(), redis_host);
    map.insert("REDIS_CACHE_PORT".to_owned(), redis_port);
    map.insert("REDIS_CACHE_USERNAME".to_owned(), redis_username);
    map.insert("REDIS_CACHE_PASSWORD".to_owned(), redis_password);
}
