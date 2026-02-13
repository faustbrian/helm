//! docker ops events module.
//!
//! Contains docker events operation used by Helm command workflows.

use anyhow::Result;

use super::common::run_docker_status;

pub(super) fn events(
    since: Option<&str>,
    until: Option<&str>,
    format: Option<&str>,
    filters: &[String],
) -> Result<()> {
    let mut args = vec!["events".to_owned()];
    if let Some(since_value) = since {
        args.push("--since".to_owned());
        args.push(since_value.to_owned());
    }
    if let Some(until_value) = until {
        args.push("--until".to_owned());
        args.push(until_value.to_owned());
    }
    if let Some(format_value) = format {
        args.push("--format".to_owned());
        args.push(format_value.to_owned());
    }
    for filter in filters {
        args.push("--filter".to_owned());
        args.push(filter.clone());
    }
    run_docker_status(&args, "Failed to execute docker events command")
}
