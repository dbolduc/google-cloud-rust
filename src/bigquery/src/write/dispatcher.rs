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
use crate::Error;
use crate::model::AppendRowsRequest;
use arc_swap::ArcSwap;
use gaxi::prost::{FromProto, ToProto};
use google_cloud_gax::error::rpc::Code;
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
}

impl Dispatcher {
    /// Creates a new `Dispatcher` for a given `StreamPool`.
    pub(crate) fn new(pool: Arc<StreamPool>) -> Self {
        let stream = pool.get();
        Self {
            pool,
            entry: ArcSwap::from_pointee(stream),
        }
    }

    /// Send the write and process the response.
    ///
    /// Evicts and updates its cached stream on transient errors.
    pub(crate) async fn send(&self, req: AppendRowsRequest) -> AppendResult<AppendResponse> {
        let req = req.to_proto().map_err(Error::ser)?;

        let stream = self.entry.load_full();
        let stream_id = stream.id;

        let resp = match stream.send(req).await {
            Ok(resp) => Ok(resp),
            Err(err) => {
                if should_reconnect(&err) {
                    // Atomically evicts failed_id and returns a new stream for use.
                    let new_stream = self.pool.evict_and_replace(stream_id);

                    // The application can `send()` multiple writes
                    // concurrently. Only one `send()` will update the cached
                    // stream on a transient error.
                    let _ = self.entry.compare_and_swap(&stream, Arc::new(new_stream));

                    // TODO(#6355): implement retries
                }
                Err(err)
            }
        }?;

        let resp = resp.cnv().map_err(Error::deser)?;
        to_result(resp)
    }
}

fn should_reconnect(err: &AppendError) -> bool {
    match err {
        AppendError::UnexpectedEndOfStream => true,
        AppendError::Rpc { source } => {
            source.is_transport()
                || source.is_io()
                || source.is_connect()
                || source
                    .status()
                    .is_some_and(|s| matches!(s.code, Code::Aborted | Code::Unavailable))
        }
        AppendError::RowErrors(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::google::cloud::bigquery::storage::v1::AppendRowsResponse;
    use crate::google::cloud::bigquery::storage::v1::append_rows_response::Response;
    use crate::write::test::*;
    use bigquery_grpc_mock::{MockBigQueryWrite, start};
    use gaxi::grpc::tonic::{Response as TonicResponse, Status as TonicStatus};
    use google_cloud_gax::error::rpc::Status as GaxStatus;
    use http::HeaderMap;
    use tokio::sync::{mpsc, oneshot};
    use tokio::task::JoinSet;

    fn test_req() -> AppendRowsRequest {
        AppendRowsRequest::new()
    }

    #[test]
    fn should_reconnect_errors() {
        assert!(should_reconnect(&AppendError::UnexpectedEndOfStream));
        assert!(should_reconnect(
            &Error::transport(HeaderMap::default(), "transport").into()
        ));
        assert!(should_reconnect(&Error::io("io").into()));
        assert!(should_reconnect(&Error::connect("connect").into()));
        assert!(should_reconnect(
            &Error::service(GaxStatus::default().set_code(Code::Aborted)).into()
        ));
        assert!(should_reconnect(
            &Error::service(GaxStatus::default().set_code(Code::Unavailable)).into()
        ));
        assert!(!should_reconnect(
            &Error::service(GaxStatus::default().set_code(Code::InvalidArgument)).into()
        ));
        assert!(!should_reconnect(
            &Error::service(GaxStatus::default().set_code(Code::ResourceExhausted)).into()
        ));
        assert!(!should_reconnect(&AppendError::RowErrors(vec![])));
    }

    #[tokio::test]
    async fn success() -> anyhow::Result<()> {
        let (response_tx, response_rx) = mpsc::channel(10);
        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .return_once(move |_| Ok(TonicResponse::from(response_rx)));

        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let transport = Arc::new(test_transport(endpoint).await?);
        let pool = Arc::new(StreamPool::new(transport, 10));
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
        let pool = Arc::new(StreamPool::new(transport, 10));
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
    async fn permanent_error() -> anyhow::Result<()> {
        let (response_tx, response_rx) = mpsc::channel(10);
        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .return_once(move |_| Ok(TonicResponse::from(response_rx)));

        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let transport = Arc::new(test_transport(endpoint).await?);
        let pool = Arc::new(StreamPool::new(transport, 10));
        let dispatcher = Arc::new(Dispatcher::new(pool.clone()));
        assert_eq!(dispatcher.entry.load().id, 1);

        let write = {
            let d = dispatcher.clone();
            tokio::spawn(async move { d.send(test_req()).await })
        };

        // Simulate a permanent stream error
        response_tx
            .send(Err(TonicStatus::failed_precondition("fail")))
            .await?;

        let err = write.await?.expect_err("should return an error");
        assert!(matches!(err, AppendError::Rpc { source: _ }));

        assert_eq!(dispatcher.entry.load().id, 1);
        assert_eq!(pool.stream_ids(), [1]);

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
        let pool = Arc::new(StreamPool::new(transport, 10));
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
        let pool = Arc::new(StreamPool::new(transport, 10));
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

    #[tokio::test]
    async fn row_error() -> anyhow::Result<()> {
        let (response_tx, response_rx) = mpsc::channel(10);
        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .return_once(move |_| Ok(TonicResponse::from(response_rx)));

        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let transport = Arc::new(test_transport(endpoint).await?);
        let pool = Arc::new(StreamPool::new(transport, 10));
        let dispatcher = Arc::new(Dispatcher::new(pool.clone()));
        assert_eq!(dispatcher.entry.load().id, 1);

        let write = {
            let d = dispatcher.clone();
            tokio::spawn(async move { d.send(test_req()).await })
        };

        let res = AppendRowsResponse {
            row_errors: vec![crate::google::cloud::bigquery::storage::v1::RowError {
                index: 0,
                code: 1,
                message: "bad row data".to_string(),
            }],
            ..Default::default()
        };
        response_tx.send(Ok(convert(&res))).await?;

        let err = write.await?.expect_err("should return an error");
        let AppendError::RowErrors(errors) = err else {
            anyhow::bail!("expected AppendError::RowErrors, got {err:?}");
        };
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].index, 0);
        assert_eq!(errors[0].message, "bad row data");

        assert_eq!(dispatcher.entry.load().id, 1);
        assert_eq!(pool.stream_ids(), [1]);

        // Verify the stream remains usable for subsequent writes.
        let write2 = {
            let d = dispatcher.clone();
            tokio::spawn(async move { d.send(test_req()).await })
        };
        response_tx.send(Ok(convert(&test_response(2)))).await?;
        let resp2 = write2.await??;
        assert_eq!(resp2.offset, Some(2));
        assert_eq!(dispatcher.entry.load().id, 1);
        assert_eq!(pool.stream_ids(), [1]);

        Ok(())
    }

    #[tokio::test]
    #[ignore = "TODO(#6355): Implement retries"]
    async fn resource_exhausted() -> anyhow::Result<()> {
        // 1. Mock server accepts 1 append_rows stream call
        // 2. Client sends write with a tracking backoff policy
        // 3. Mock sends response with Response::Error(Status { code: ResourceExhausted })
        // 4. Verify stream was NOT evicted: pool.stream_ids() == [1]
        // 5. Mock sends second response on the SAME stream: Ok(AppendResult)
        // 6. Verify write succeeds and backoff was called once
        todo!()
    }

    #[tokio::test]
    #[ignore = "TODO(#6355): Implement retries"]
    async fn unexpected_end_of_stream() -> anyhow::Result<()> {
        // 1. Mock expects stream 1 and stream 2
        // 2. Client sends write to stream 1 (id: 1)
        // 3. Drop response channel 1 (simulating stream close / UnexpectedEndOfStream)
        // 4. Stream 2 is opened automatically by the pool
        // 5. Stream 2 sends Ok(AppendResult)
        // 6. Verify write succeeds with new stream id: 2
        // 7. Verify retry was immediate (0 backoff delay)
        todo!()
    }

    #[tokio::test]
    #[ignore = "TODO(#6355): Implement retries"]
    async fn consecutive_disconnects() -> anyhow::Result<()> {
        // 1. Mock expects streams 1, 2, and 3
        // 2. Client sends write with a tracking backoff policy
        // 3. Drop stream 1 -> client retries immediately on stream 2 (0 delay)
        // 4. Drop stream 2 -> client encounters consecutive disconnect
        // 5. Verify backoff sleep was invoked before attempting stream 3
        // 6. Stream 3 sends Ok(AppendResult)
        // 7. Verify write succeeds
        todo!()
    }

    #[tokio::test]
    #[ignore = "TODO(#6355): Implement retries"]
    async fn transport_error() -> anyhow::Result<()> {
        // 1. Mock expects stream 1 and stream 2
        // 2. Client sends write
        // 3. Stream 1 yields TonicStatus with a source error (h2 reset)
        // 4. Verify stream 1 is evicted (id -> 2)
        // 5. Stream 2 yields Ok(AppendResult)
        // 6. Verify write succeeds immediately without backoff delay
        todo!()
    }

    #[tokio::test]
    #[ignore = "TODO(#6355): Implement retries"]
    async fn server_restart() -> anyhow::Result<()> {
        // 1. Mock expects stream 1 and stream 2
        // 2. Client sends write
        // 3. Stream 1 yields TonicStatus::aborted("Closing the stream because server is restarted")
        // 4. Verify stream 1 is evicted (pool id: 2)
        // 5. Stream 2 yields Ok(AppendResult)
        // 6. Verify write succeeds immediately without backoff delay
        todo!()
    }

    #[tokio::test]
    #[ignore = "TODO(#6355): Implement retries"]
    async fn shared_error() -> anyhow::Result<()> {
        todo!()
    }

    #[tokio::test]
    #[ignore = "TODO(#6355): Implement retries"]
    async fn connect_error() -> anyhow::Result<()> {
        // 1. Mock expect_append_rows call 1: returns Err(TonicStatus::unavailable("unavailable"))
        // 2. Mock expect_append_rows call 2: succeeds and returns response channel
        // 3. Client sends write with a tracking backoff policy
        // 4. Verify backoff was invoked between call 1 and call 2
        // 5. Stream 2 yields Ok(AppendResult)
        // 6. Verify write succeeds
        todo!()
    }

    #[tokio::test]
    #[ignore = "TODO(#6355): Implement retries"]
    async fn attempt_timeout() -> anyhow::Result<()> {
        // 1. Configure dispatcher with attempt_timeout (e.g. 50ms)
        // 2. Mock expects stream 1 and stream 2
        // 3. Client sends write to stream 1
        // 4. Stream 1 never sends a response (hangs)
        // 5. After 50ms, attempt_timeout triggers
        // 6. Verify stream 1 is evicted (id -> 2)
        // 7. Stream 2 sends Ok(AppendResult)
        // 8. Verify write succeeds on stream 2
        todo!()
    }

    #[tokio::test]
    #[ignore = "TODO(#6355): Implement retries"]
    async fn retry_exhausted() -> anyhow::Result<()> {
        // 1. Configure retry policy with with_time_limit(100ms) and attempt_timeout(40ms)
        // 2. Mock streams always hang or drop
        // 3. Client sends write
        // 4. Loop attempts retry until elapsed time >= 100ms
        // 5. Verify write returns Err(AppendError::Rpc { source }) where source.is_exhausted() == true
        todo!()
    }

    #[tokio::test]
    #[ignore = "TODO(#6355): Implement retries"]
    async fn strict_customer_policy() -> anyhow::Result<()> {
        // 1. Configure customer retry policy that returns Permanent for all RPC errors
        // 2. Stream 1 closes (drop response_tx)
        // 3. Stream 2 succeeds with Ok(AppendResult)
        // 4. Verify write succeeds: the library handled UnexpectedEndOfStream internally
        todo!()
    }

    #[tokio::test]
    #[ignore = "TODO(#6355): Implement retries"]
    async fn customer_retry_policy() -> anyhow::Result<()> {
        // 1. Configure customer retry policy that rejects Code::ResourceExhausted
        // 2. Stream yields response with error Code::ResourceExhausted
        // 3. Verify write fails immediately with Code::ResourceExhausted (no retries)
        // 4. Verify stream was kept open (id == 1)
        todo!()
    }

    #[tokio::test]
    async fn permanent_rpc_error() -> anyhow::Result<()> {
        let (response_tx, response_rx) = mpsc::channel(10);
        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .return_once(move |_| Ok(TonicResponse::from(response_rx)));

        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let transport = Arc::new(test_transport(endpoint).await?);
        let pool = Arc::new(StreamPool::new(transport, 10));
        let dispatcher = Arc::new(Dispatcher::new(pool.clone()));
        assert_eq!(dispatcher.entry.load().id, 1);

        let write = {
            let d = dispatcher.clone();
            tokio::spawn(async move { d.send(test_req()).await })
        };

        let res = AppendRowsResponse {
            response: Some(Response::Error(crate::google::rpc::Status {
                code: Code::InvalidArgument as i32,
                message: "table does not exist".to_string(),
                details: vec![],
            })),
            ..Default::default()
        };
        response_tx.send(Ok(convert(&res))).await?;

        let err = write.await?.expect_err("should return an error");
        let AppendError::Rpc { source } = err else {
            anyhow::bail!("expected AppendError::Rpc, got {err:?}");
        };
        let status = source.status().expect("status should be set");
        assert_eq!(status.code, Code::InvalidArgument);
        assert_eq!(status.message, "table does not exist");

        assert_eq!(dispatcher.entry.load().id, 1);
        assert_eq!(pool.stream_ids(), [1]);

        // Verify the stream remains usable for subsequent writes.
        let write2 = {
            let d = dispatcher.clone();
            tokio::spawn(async move { d.send(test_req()).await })
        };
        response_tx.send(Ok(convert(&test_response(2)))).await?;
        let resp2 = write2.await??;
        assert_eq!(resp2.offset, Some(2));
        assert_eq!(dispatcher.entry.load().id, 1);
        assert_eq!(pool.stream_ids(), [1]);

        Ok(())
    }
}
