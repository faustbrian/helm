use futures_util::stream::{self, StreamExt};
use std::future::Future;
use std::num::NonZeroUsize;
use std::time::Duration;

/// Converges independent resources while yielding to a bounded control-plane
/// heartbeat throughout slow Engine operations.
pub(crate) async fn run_bounded_independent_reconciliation_with_heartbeat<
    Input,
    Output,
    Operation,
    OperationFuture,
    Heartbeat,
>(
    inputs: impl IntoIterator<Item = Input>,
    concurrency: NonZeroUsize,
    heartbeat_interval: Duration,
    operation: Operation,
    mut heartbeat: Heartbeat,
) -> Vec<Output>
where
    Operation: Fn(Input) -> OperationFuture,
    OperationFuture: Future<Output = Output>,
    Heartbeat: FnMut(),
{
    let mut pending = stream::iter(inputs.into_iter().enumerate().map(|(index, input)| {
        let future = operation(input);

        async move { (index, future.await) }
    }))
    .buffer_unordered(concurrency.get());
    let heartbeat_interval = heartbeat_interval.max(Duration::from_millis(1));
    let mut results = Vec::new();

    loop {
        match tokio::time::timeout(heartbeat_interval, pending.next()).await {
            Ok(Some(result)) => results.push(result),
            Ok(None) => break,
            Err(_) => heartbeat(),
        }
    }
    results.sort_unstable_by_key(|(index, _)| *index);

    results.into_iter().map(|(_, output)| output).collect()
}

#[cfg(test)]
mod tests {
    use super::run_bounded_independent_reconciliation_with_heartbeat;
    use std::num::NonZeroUsize;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn slow_reconciliation_keeps_heartbeats_and_plan_order() {
        let heartbeats = Arc::new(AtomicUsize::new(0));
        let observed_heartbeats = Arc::clone(&heartbeats);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");

        let results = runtime.block_on(run_bounded_independent_reconciliation_with_heartbeat(
            [("slow", 80), ("fast", 1)],
            NonZeroUsize::new(2).expect("non-zero concurrency"),
            Duration::from_millis(10),
            |(name, delay_milliseconds)| async move {
                tokio::time::sleep(Duration::from_millis(delay_milliseconds)).await;
                name
            },
            move || {
                observed_heartbeats.fetch_add(1, Ordering::Relaxed);
            },
        ));

        assert_eq!(results, ["slow", "fast"]);
        assert!(heartbeats.load(Ordering::Relaxed) >= 3);
    }
}
