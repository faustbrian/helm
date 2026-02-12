use crate::config::ServiceConfig;

mod entrypoint;
mod env;

use entrypoint::append_entrypoint_args;
use env::append_run_options;

pub(super) fn build_run_args(service: &ServiceConfig, container_name: &str) -> Vec<String> {
    let mut args = vec![
        "run".to_owned(),
        "-d".to_owned(),
        "--name".to_owned(),
        container_name.to_owned(),
        "-p".to_owned(),
        format!(
            "{}:{}:{}",
            service.host,
            service.port,
            service.default_port()
        ),
    ];

    append_run_options(&mut args, service);
    args.push(service.image.clone());
    append_entrypoint_args(&mut args, service);
    args
}
