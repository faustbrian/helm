use crate::control_plane::{
    IpcError, IpcPayload, IpcRequest, default_docker_socket, default_unix_daemon_runtime_directory,
    resolve_registry_image_references, send_unix_request,
};
use anyhow::Result;
use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::time::Instant;

use super::{
    ENGINE_CONNECT_TIMEOUT, ENGINE_RETRY_INTERVAL, REQUEST_TIMEOUT, engine_is_reconnecting,
    next_request_id, resolved_references,
};

pub(super) enum DaemonResolution {
    Resolved(BTreeMap<String, String>),
    Unavailable,
}

pub(super) fn resolve_image_references(
    references: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>> {
    resolve_with_daemon_fallback(
        || resolve_through_daemon(references),
        || {
            let socket = default_docker_socket()?;
            resolve_registry_image_references(&socket, references).map_err(Into::into)
        },
    )
}

fn resolve_with_daemon_fallback<Daemon, Direct>(
    daemon: Daemon,
    direct: Direct,
) -> Result<BTreeMap<String, String>>
where
    Daemon: FnOnce() -> Result<DaemonResolution>,
    Direct: FnOnce() -> Result<BTreeMap<String, String>>,
{
    match daemon()? {
        DaemonResolution::Resolved(references) => Ok(references),
        DaemonResolution::Unavailable => direct(),
    }
}

fn resolve_through_daemon(references: &BTreeMap<String, String>) -> Result<DaemonResolution> {
    let socket_path = default_unix_daemon_runtime_directory()?.join("daemon.sock");
    let deadline = Instant::now() + ENGINE_CONNECT_TIMEOUT;
    loop {
        let response = match send_unix_request(
            &socket_path,
            &IpcRequest::new(
                next_request_id(),
                IpcPayload::ResolveImageReferences {
                    references: references.clone(),
                },
            ),
            REQUEST_TIMEOUT,
        ) {
            Ok(response) => response,
            Err(error) if daemon_is_unavailable(&error) => {
                return Ok(DaemonResolution::Unavailable);
            }
            Err(error) => return Err(error.into()),
        };
        if !engine_is_reconnecting(response.outcome()) || Instant::now() >= deadline {
            return resolved_references(response.outcome()).map(DaemonResolution::Resolved);
        }
        std::thread::sleep(ENGINE_RETRY_INTERVAL);
    }
}

fn daemon_is_unavailable(error: &IpcError) -> bool {
    matches!(
        error,
        IpcError::EndpointIo { source, .. }
            if matches!(source.kind(), ErrorKind::NotFound | ErrorKind::ConnectionRefused)
    )
}

#[cfg(test)]
mod tests {
    use super::{DaemonResolution, daemon_is_unavailable, resolve_with_daemon_fallback};
    use crate::control_plane::IpcError;
    use anyhow::bail;
    use std::collections::BTreeMap;
    use std::io::{Error, ErrorKind};
    use std::path::PathBuf;

    fn resolved(reference: &str) -> BTreeMap<String, String> {
        BTreeMap::from([("app".to_owned(), reference.to_owned())])
    }

    #[test]
    fn available_daemon_remains_the_authoritative_resolver() {
        let expected = resolved("app@sha256:daemon");

        let actual = resolve_with_daemon_fallback(
            || Ok(DaemonResolution::Resolved(expected.clone())),
            || bail!("direct resolver must not run"),
        )
        .expect("daemon resolution");

        assert_eq!(actual, expected);
    }

    #[test]
    fn unavailable_daemon_uses_the_bootstrap_resolver() {
        let expected = resolved("app@sha256:direct");

        let actual = resolve_with_daemon_fallback(
            || Ok(DaemonResolution::Unavailable),
            || Ok(expected.clone()),
        )
        .expect("direct bootstrap resolution");

        assert_eq!(actual, expected);
    }

    #[test]
    fn daemon_protocol_failures_do_not_silently_fall_back() {
        let error = resolve_with_daemon_fallback(
            || bail!("invalid daemon response"),
            || Ok(resolved("app@sha256:direct")),
        )
        .expect_err("daemon protocol failure");

        assert_eq!(error.to_string(), "invalid daemon response");
    }

    #[test]
    fn only_absent_daemon_endpoints_enable_bootstrap_resolution() {
        for kind in [ErrorKind::NotFound, ErrorKind::ConnectionRefused] {
            let error = endpoint_error(kind);

            assert!(daemon_is_unavailable(&error));
        }
    }

    #[test]
    fn permission_failures_do_not_bypass_the_daemon_endpoint() {
        assert!(!daemon_is_unavailable(&endpoint_error(
            ErrorKind::PermissionDenied
        )));
    }

    fn endpoint_error(kind: ErrorKind) -> IpcError {
        IpcError::EndpointIo {
            path: PathBuf::from("/tmp/stackctl-daemon.sock"),
            source: Error::from(kind),
        }
    }
}
