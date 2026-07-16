use super::EngineError;
use std::future::Future;
use std::time::Duration;

/// Repeats a bounded Engine transport operation before preserving its error.
pub(super) async fn retry_transient_engine_operation<Operation, OperationFuture, Output>(
    operation: &mut Operation,
    attempts: usize,
    delay: Duration,
) -> Result<Output, EngineError>
where
    Operation: FnMut() -> OperationFuture,
    OperationFuture: Future<Output = Result<Output, EngineError>>,
{
    if attempts == 0 {
        return Err(EngineError::InvalidRequest {
            detail: "transient Engine operation requires at least one attempt".to_owned(),
        });
    }

    for attempt in 1..=attempts {
        match operation().await {
            Ok(output) => return Ok(output),
            Err(_) if attempt < attempts => tokio::time::sleep(delay).await,
            Err(error) => return Err(error),
        }
    }

    unreachable!("the final transient Engine operation attempt always returns")
}

#[cfg(test)]
mod tests {
    use super::retry_transient_engine_operation;
    use crate::control_plane::engine::EngineError;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn retries_transient_engine_failures_until_the_bounded_success() {
        let calls = Arc::new(AtomicUsize::new(0));
        let observed_calls = Arc::clone(&calls);
        let mut operation = move || {
            let calls = Arc::clone(&calls);

            async move {
                let call = calls.fetch_add(1, Ordering::SeqCst);
                if call < 2 {
                    return Err(EngineError::Backend {
                        detail: "truncated Engine stream".to_owned(),
                    });
                }

                Ok("pulled")
            }
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("build retry test runtime");

        let result = runtime.block_on(retry_transient_engine_operation(
            &mut operation,
            3,
            Duration::ZERO,
        ));

        assert_eq!(result.expect("third pull succeeds"), "pulled");
        assert_eq!(observed_calls.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn preserves_the_final_error_after_the_attempt_budget() {
        let calls = Arc::new(AtomicUsize::new(0));
        let observed_calls = Arc::clone(&calls);
        let mut operation = move || {
            let calls = Arc::clone(&calls);

            async move {
                let call = calls.fetch_add(1, Ordering::SeqCst) + 1;
                Err::<(), _>(EngineError::Backend {
                    detail: format!("pull failure {call}"),
                })
            }
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("build retry test runtime");

        let error = runtime
            .block_on(retry_transient_engine_operation(
                &mut operation,
                3,
                Duration::ZERO,
            ))
            .expect_err("attempt budget preserves its final failure");

        assert_eq!(
            error,
            EngineError::Backend {
                detail: "pull failure 3".to_owned(),
            }
        );
        assert_eq!(observed_calls.load(Ordering::SeqCst), 3);
    }
}
