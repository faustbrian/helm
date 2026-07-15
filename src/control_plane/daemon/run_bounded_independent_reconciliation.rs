use futures_util::stream::{self, StreamExt};
use std::future::Future;
use std::num::NonZeroUsize;

/// Converges independent resources concurrently while preserving plan order.
pub(crate) async fn run_bounded_independent_reconciliation<
    Input,
    Output,
    Operation,
    OperationFuture,
>(
    inputs: impl IntoIterator<Item = Input>,
    concurrency: NonZeroUsize,
    operation: Operation,
) -> Vec<Output>
where
    Operation: Fn(Input) -> OperationFuture,
    OperationFuture: Future<Output = Output>,
{
    stream::iter(inputs.into_iter().map(operation))
        .buffered(concurrency.get())
        .collect()
        .await
}

#[cfg(test)]
mod tests {
    use super::run_bounded_independent_reconciliation;
    use std::num::NonZeroUsize;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn independent_reconciliation_is_bounded_and_returns_plan_order() {
        let active = Arc::new(AtomicUsize::new(0));
        let maximum_active = Arc::new(AtomicUsize::new(0));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");

        let results = runtime.block_on(run_bounded_independent_reconciliation(
            [("slow", 30), ("fast", 1), ("middle", 10)],
            NonZeroUsize::new(2).expect("non-zero concurrency"),
            |(name, delay_milliseconds)| {
                let active = Arc::clone(&active);
                let maximum_active = Arc::clone(&maximum_active);

                async move {
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    maximum_active.fetch_max(current, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(delay_milliseconds)).await;
                    active.fetch_sub(1, Ordering::SeqCst);

                    name
                }
            },
        ));

        assert_eq!(results, ["slow", "fast", "middle"]);
        assert_eq!(maximum_active.load(Ordering::SeqCst), 2);
    }
}
