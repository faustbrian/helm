use super::EngineError;
use std::future::Future;
use std::time::Duration;

/// Applies a cancellable deadline to one direct Engine operation.
pub(super) async fn bounded_engine_operation<Output>(
    action: &'static str,
    deadline: Duration,
    operation: impl Future<Output = Result<Output, EngineError>>,
) -> Result<Output, EngineError> {
    match tokio::time::timeout(deadline, operation).await {
        Ok(result) => result,
        Err(_) => Err(EngineError::Timeout {
            action: action.to_owned(),
            timeout_milliseconds: u64::try_from(deadline.as_millis()).unwrap_or(u64::MAX),
        }),
    }
}
