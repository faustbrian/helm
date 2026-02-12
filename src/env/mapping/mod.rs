use std::collections::HashMap;

use crate::config::ServiceConfig;

mod drivers;
mod remap;

use drivers::base_map_for_driver;
use remap::apply_env_remapping;

pub(super) fn build_laravel_env_map(service: &ServiceConfig) -> HashMap<String, String> {
    let mut vars = base_map_for_driver(service);
    apply_env_remapping(&mut vars, service.env_mapping.as_ref());
    vars
}
