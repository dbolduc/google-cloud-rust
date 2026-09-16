// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::append_response::{AppendResponse, to_result};
use super::entry::StreamEntry;
use super::error::{AppendError, AppendResult};
use super::pool::StreamPool;
use super::retry_policy::RetryableErrors;
use crate::Error;
use crate::google::cloud::bigquery::storage::v1::AppendRowsRequest as AppendRowsRequestProto;
use crate::model::AppendRowsRequest;
use arc_swap::ArcSwap;
use gaxi::prost::{FromProto, ToProto};
use google_cloud_gax::backoff_policy::BackoffPolicy;
use google_cloud_gax::exponential_backoff::ExponentialBackoff;
use google_cloud_gax::retry_policy::{RetryPolicy, RetryPolicyExt};
use google_cloud_gax::retry_result::RetryResult;
use google_cloud_gax::retry_state::RetryState;
use std::sync::Arc;

/// Efficiently dispatches writes to a stream in a stream pool.
///
/// This struct caches a `StreamEntry` and loads it atomically on each write.
///
/// On a transient error, the `Dispatcher` notifies the `StreamPool` of the
/// failed `StreamEntry` and receives a new `StreamEntry` to use for future
/// writes.
///
/// This struct is also responsible for retrying individual writes.
#[derive(Debug)]
pub(crate) struct Dispatcher {
    pub(crate) pool: Arc<StreamPool>,
    pub(crate) entry: ArcSwap<StreamEntry>,
    retry_policy: Arc<dyn RetryPolicy>,
    backoff_policy: Arc<dyn BackoffPolicy>,
}

impl Dispatcher {
    /// Creates a new `Dispatcher` for a given `StreamPool`.
    pub(crate) fn new(pool: Arc<StreamPool>) -> Self {
        // TODO(#6355): plumb the policies from the client
        Self::with_policies(
            pool,
            Arc::new(RetryableErrors.with_attempt_limit(3)),
            Arc::new(ExponentialBackoff::default()),
        )
    }

    /// Creates a new `Dispatcher` with the given retry and backoff policies.
    pub(super) fn with_policies(
        pool: Arc<StreamPool>,
        retry_policy: Arc<dyn RetryPolicy>,
        backoff_policy: Arc<dyn BackoffPolicy>,
    ) -> Self {
        let stream = pool.get();
        Self {
            pool,
            entry: ArcSwap::from_pointee(stream),
            retry_policy,
            backoff_policy,
        }
    }

    /// Send the write and process the response.
    ///
    /// Evicts and updates its cached stream on terminal stream errors.
    pub(crate) async fn send(&self, req: AppendRowsRequest) -> AppendResult<AppendResponse> {
        let req = req.to_proto().map_err(Error::ser)?;

        // The default stream has at-least-once semantics, so all writes are
        // idempotent.
        let mut state = RetryState::new(true);
        loop {
            state.attempt_count += 1;
            let err = match self.send_one_attempt(req.clone()).await {
                Ok(resp) => return Ok(resp),
                Err(err) => err,
            };
            match err {
                // RowErrors are always permanent.
                AppendError::RowErrors(_) => return Err(err),

                // Adapt the error into a `gax::Error::io()`. This lets us reuse
                // the standard gax retry and backoff policy interfaces.
                //
                // We could always retry these requests, but that may be
                // surprising to an application that supplies a policy with an
                // attempt limit.
                AppendError::UnexpectedEndOfStream => {
                    let err = Error::io(AppendError::UnexpectedEndOfStream);
                    match self.retry_policy.on_error(&state, err) {
                        RetryResult::Continue(_) => {}
                        RetryResult::Exhausted(_) | RetryResult::Permanent(_) => {
                            // Return the original error.
                            return Err(AppendError::UnexpectedEndOfStream);
                        }
                    }
                }

                AppendError::Rpc { source } => match self.retry_policy.on_error(&state, source) {
                    RetryResult::Continue(_) => {}
                    RetryResult::Exhausted(e) | RetryResult::Permanent(e) => {
                        return Err(e.into());
                    }
                },
            }
            tokio::time::sleep(self.backoff_policy.on_failure(&state)).await;
        }
    }

    /// Makes one attempt to send the write and process the response.
    ///
    /// Evicts the cached stream if the attempt fails.
    async fn send_one_attempt(&self, req: AppendRowsRequestProto) -> AppendResult<AppendResponse> {
        let stream = self.entry.load_full();
        let stream_id = stream.id;

        let resp = match stream.send(req).await {
            Ok(resp) => resp,
            Err(err) => {
                // Any error here means the stream is dead. Either the runner
                // task exited (`UnexpectedEndOfStream`), or it forwarded a
                // stream-level gRPC error. Note that `AppendError::RowErrors`
                // cannot appear here. It is produced by `to_result()` below.
                //
                // It is fine to replace the stream entry on a typically
                // permanent error, as streams are lazily initialized.

                // Atomically evicts failed_id and returns a new stream for use.
                let new_stream = self.pool.evict_and_replace(stream_id);

                // The application can `send()` multiple writes concurrently.
                // Only one `send()` will update the cached stream.
                let _ = self.entry.compare_and_swap(&stream, Arc::new(new_stream));

                return Err(err);
            }
        };

        let resp = resp.cnv().map_err(Error::deser)?;
        to_result(resp)
    }
}

#[cfg(test)]
mod tests {
    use super::super::error::AppendError;
    use super::super::pool::StreamPoolOptions;
    use super::*;
    use crate::write::test::*;
    use bigquery_grpc_mock::{MockBigQueryWrite, start};
    use gaxi::grpc::tonic::{Response as TonicResponse, Status as TonicStatus};
    use tokio::sync::{mpsc, oneshot};
    use tokio::task::JoinSet;

    fn test_req() -> AppendRowsRequest {
        AppendRowsRequest::new()
    }

    #[tokio::test]
    async fn success() -> anyhow::Result<()> {
        let (response_tx, response_rx) = mpsc::channel(10);
        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .return_once(move |_| Ok(TonicResponse::from(response_rx)));

        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let transport = Arc::new(test_transport(endpoint).await?);
        let pool = Arc::new(StreamPool::new(transport, StreamPoolOptions::default()));
        let dispatcher = Arc::new(Dispatcher::new(pool));
        assert_eq!(dispatcher.entry.load().id, 1);

        let write1 = {
            let d = dispatcher.clone();
            tokio::spawn(async move { d.send(test_req()).await })
        };
        let write2 = {
            let d = dispatcher.clone();
            tokio::spawn(async move { d.send(test_req()).await })
        };

        // Respond to the writes
        response_tx.send(Ok(convert(&test_response(1)))).await?;
        assert_eq!(write1.await??.offset, Some(1));

        response_tx.send(Ok(convert(&test_response(2)))).await?;
        assert_eq!(write2.await??.offset, Some(2));

        // Verify we are still on the same stream.
        assert_eq!(dispatcher.entry.load().id, 1);

        Ok(())
    }

    #[tokio::test]
    async fn stream_closed() -> anyhow::Result<()> {
        let (response_tx, response_rx) = mpsc::channel(10);
        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .return_once(move |_| Ok(TonicResponse::from(response_rx)));

        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let transport = Arc::new(test_transport(endpoint).await?);
        let pool = Arc::new(StreamPool::new(transport, StreamPoolOptions::default()));
        let dispatcher = Arc::new(Dispatcher::new(pool));
        assert_eq!(dispatcher.entry.load().id, 1);

        let write = {
            let d = dispatcher.clone();
            tokio::spawn(async move { d.send(test_req()).await })
        };

        // Simulate the stream closing before responding to the request.
        drop(response_tx);

        // TODO(#6355) - expect retries.
        let err = write.await?.expect_err("should return an error");
        assert!(matches!(err, AppendError::UnexpectedEndOfStream));

        // We ran into a transient error. We should now have a new stream.
        assert_eq!(dispatcher.entry.load().id, 2);

        Ok(())
    }

    #[tokio::test]
    async fn rpc_error_evicts_stream() -> anyhow::Result<()> {
        let (response_tx, response_rx) = mpsc::channel(10);
        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .return_once(move |_| Ok(TonicResponse::from(response_rx)));

        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let transport = Arc::new(test_transport(endpoint).await?);
        let pool = Arc::new(StreamPool::new(transport, StreamPoolOptions::default()));
        let dispatcher = Arc::new(Dispatcher::new(pool.clone()));
        assert_eq!(dispatcher.entry.load().id, 1);

        let write = {
            let d = dispatcher.clone();
            tokio::spawn(async move { d.send(test_req()).await })
        };

        // Simulate a stream-level error. The error is not retryable, but the
        // stream is still dead.
        response_tx
            .send(Err(TonicStatus::failed_precondition("fail")))
            .await?;

        let err = write.await?.expect_err("should return an error");
        assert!(matches!(err, AppendError::Rpc { source: _ }));

        // The stream terminated, so it should not remain in the pool.
        assert_eq!(dispatcher.entry.load().id, 2);
        assert_eq!(pool.stream_ids(), [2]);

        Ok(())
    }

    #[tokio::test]
    async fn transient_error_evict_contention() -> anyhow::Result<()> {
        let (response_tx, response_rx) = mpsc::channel(10);
        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .return_once(move |_| Ok(TonicResponse::from(response_rx)));

        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let transport = Arc::new(test_transport(endpoint).await?);
        let pool = Arc::new(StreamPool::new(transport, StreamPoolOptions::default()));
        let dispatcher = Arc::new(Dispatcher::new(pool.clone()));
        assert_eq!(dispatcher.entry.load().id, 1);

        let mut writes = JoinSet::new();
        for _ in 0..1000 {
            let d = dispatcher.clone();
            writes.spawn(async move { d.send(test_req()).await });
        }

        // Simulate the stream closing before responding to the requests.
        drop(response_tx);

        while let Some(write) = writes.join_next().await {
            let err = write?.expect_err("should return an error");
            assert!(matches!(err, AppendError::UnexpectedEndOfStream));
        }

        // We ran into a transient error. We should now have a new stream. Only
        // one of the callers should have evicted the failed stream.
        assert_eq!(dispatcher.entry.load().id, 2);
        assert_eq!(pool.stream_ids(), [2]);

        Ok(())
    }

    #[tokio::test]
    async fn writes_bypass_pool_lock() -> anyhow::Result<()> {
        let (response_tx, response_rx) = mpsc::channel(10);
        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .return_once(move |_| Ok(TonicResponse::from(response_rx)));

        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let transport = Arc::new(test_transport(endpoint).await?);
        let pool = Arc::new(StreamPool::new(transport, StreamPoolOptions::default()));
        let dispatcher = Arc::new(Dispatcher::new(pool.clone()));

        // Acquire the stream pool's lock to simulate a pool scaling event. This
        // needs to run in a separate thread because we don't want to hold the
        // `std::sync::MutexGuard` across `await` points.
        let (lock_acquired_tx, lock_acquired_rx) = oneshot::channel();
        let (release_lock_tx, release_lock_rx) = std::sync::mpsc::channel::<()>();
        std::thread::spawn(move || {
            let _guard = pool.lock();
            let _ = lock_acquired_tx.send(());
            let _ = release_lock_rx.recv();
        });

        // Wait until the lock is acquired to send a write.
        lock_acquired_rx.await?;
        let write = tokio::spawn(async move { dispatcher.send(test_req()).await });

        // Verify the write goes through, even with the pool's lock held.
        response_tx.send(Ok(convert(&test_response(1)))).await?;
        assert_eq!(write.await??.offset, Some(1));

        // Release the lock
        drop(release_lock_tx);

        Ok(())
    }

    /// The write is retried when the stream closes before responding.
    #[tokio::test]
    async fn retry_after_stream_closed() -> anyhow::Result<()> {
        todo!()
    }

    /// A policy that stops the loop is honored on a closed stream, and the
    /// original `UnexpectedEndOfStream` is reported.
    #[tokio::test]
    async fn stream_closed_respects_retry_policy() -> anyhow::Result<()> {
        todo!()
    }

    /// The write is retried when the policy classifies the error as transient.
    #[tokio::test]
    async fn retry_after_transient_rpc_error() -> anyhow::Result<()> {
        todo!()
    }

    /// Row errors are returned immediately, and the stream is not evicted.
    #[tokio::test]
    async fn row_errors_not_retried() -> anyhow::Result<()> {
        todo!()
    }

    /// The last error is reported once the policy stops the loop.
    #[tokio::test]
    async fn retry_exhausted() -> anyhow::Result<()> {
        todo!()
    }

    /// The backoff policy is consulted between attempts.
    #[tokio::test]
    async fn backoff_between_attempts() -> anyhow::Result<()> {
        todo!()
    }
}
