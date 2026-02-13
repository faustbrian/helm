//! docker ops cp module.
//!
//! Contains docker cp operation used by Helm command workflows.

use anyhow::Result;

use super::common::run_docker_status;

pub(super) fn cp(source: &str, destination: &str, follow_link: bool, archive: bool) -> Result<()> {
    let mut args = vec!["cp".to_owned()];
    if follow_link {
        args.push("-L".to_owned());
    }
    if archive {
        args.push("-a".to_owned());
    }
    args.push(source.to_owned());
    args.push(destination.to_owned());
    run_docker_status(&args, "Failed to execute docker cp command")
}
