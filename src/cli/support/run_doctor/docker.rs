use std::process::Command;

use crate::output::{self, LogLevel, Persistence};

pub(super) fn check_docker_availability() -> bool {
    let mut has_error = false;

    let docker_version = Command::new("docker").arg("--version").output();
    match docker_version {
        Ok(output) if output.status.success() => output::event(
            "doctor",
            LogLevel::Success,
            "Docker CLI available",
            Persistence::Persistent,
        ),
        Ok(output) => {
            has_error = true;
            let stderr = String::from_utf8_lossy(&output.stderr);
            output::event(
                "doctor",
                LogLevel::Error,
                &format!("Docker unavailable: {stderr}"),
                Persistence::Persistent,
            );
        }
        Err(err) => {
            has_error = true;
            output::event(
                "doctor",
                LogLevel::Error,
                &format!("Docker unavailable: {err}"),
                Persistence::Persistent,
            );
        }
    }

    let docker_info = Command::new("docker").arg("info").output();
    match docker_info {
        Ok(output) if output.status.success() => output::event(
            "doctor",
            LogLevel::Success,
            "Docker daemon reachable",
            Persistence::Persistent,
        ),
        Ok(output) => {
            has_error = true;
            let stderr = String::from_utf8_lossy(&output.stderr);
            output::event(
                "doctor",
                LogLevel::Error,
                &format!("Docker daemon not reachable: {stderr}"),
                Persistence::Persistent,
            );
        }
        Err(err) => {
            has_error = true;
            output::event(
                "doctor",
                LogLevel::Error,
                &format!("Docker info failed: {err}"),
                Persistence::Persistent,
            );
        }
    }

    has_error
}
