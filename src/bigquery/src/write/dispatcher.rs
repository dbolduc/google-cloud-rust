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
use google_cloud_gax::backoff_policy::BackoffPolicy;
use google_cloud_gax::error::rpc::Code;
use google_cloud_gax::retry_policy::RetryPolicy;
use google_cloud_gax::retry_result::RetryResult;
use google_cloud_gax::retry_state::RetryState;
use std::sync::Arc;
use std::time::Duration;

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
    pub(crate) retry_policy: Arc<dyn RetryPolicy>,
    pub(crate) backoff_policy: Arc<dyn BackoffPolicy>,
    pub(crate) attempt_timeout: Option<Duration>,
}

impl Dispatcher {
    /// Creates a new `Dispatcher` for a given `StreamPool`.
    pub(crate) fn new(
        pool: Arc<StreamPool>,
        retry_policy: Arc<dyn RetryPolicy>,
        backoff_policy: Arc<dyn BackoffPolicy>,
        attempt_timeout: Option<Duration>,
    ) -> Self {
        let stream = pool.get();
        Self {
            pool,
            entry: ArcSwap::from_pointee(stream),
            retry_policy,
            backoff_policy,
            attempt_timeout,
        }
    }

    /// Send the write and process the response.
    ///
    /// Evicts and updates its cached stream on transient errors.
    pub(crate) async fn send(&self, req: AppendRowsRequest) -> AppendResult<AppendResponse> {
        let req = req.to_proto().map_err(Error::ser)?;
        let mut state = RetryState::new(true);

        loop {
            let remaining_time = self.retry_policy.remaining_time(&state);
            if remaining_time.is_some_and(|r| r.is_zero()) {
                return Err(AppendError::Rpc {
                    source: Error::exhausted("retry policy exhausted"),
                });
            }

            state.attempt_count += 1;
            let effective_timeout = resolve_effective_timeout(self.attempt_timeout, remaining_time);

            let (res, stream_id, reconnected) =
                self.send_one_attempt(req.clone(), effective_timeout).await;
            let err = match res {
                Ok(res) => return Ok(res),
                Err(e) => {
                    let now = time::OffsetDateTime::now_utc();
                    let attempt = state.attempt_count;
                    println!(
                        "# [{now}] INTERNAL ERROR (stream_id: {stream_id}, attempt: {attempt}, reconnect: {reconnected}): {e:?}"
                    );
                    e
                }
            };

            let (delay, source) = match err {
                AppendError::RowErrors(_) => return Err(err),
                AppendError::UnexpectedEndOfStream => {
                    let delay = self.backoff_policy.on_failure(&state);
                    (delay, Error::io("unexpected end of stream"))
                }
                AppendError::Rpc { source } => match self.retry_policy.on_error(&state, source) {
                    RetryResult::Continue(source) => {
                        let delay = self.backoff_policy.on_failure(&state);
                        (delay, source)
                    }
                    RetryResult::Exhausted(source) => {
                        let source = if source.is_exhausted() {
                            source
                        } else {
                            Error::exhausted(source)
                        };
                        return Err(AppendError::Rpc { source });
                    }
                    RetryResult::Permanent(source) => {
                        return Err(AppendError::Rpc { source });
                    }
                },
            };

            let remaining_time = self.retry_policy.remaining_time(&state);
            if remaining_time.is_some_and(|remaining| remaining < delay) {
                return Err(AppendError::Rpc {
                    source: Error::exhausted(source),
                });
            }

            tokio::time::sleep(delay).await;
        }
    }

    async fn send_one_attempt(
        &self,
        req: crate::google::cloud::bigquery::storage::v1::AppendRowsRequest,
        effective_timeout: Option<Duration>,
    ) -> (AppendResult<AppendResponse>, u64, bool) {
        let stream = self.entry.load_full();
        let stream_id = stream.id;

        let send_fut = stream.send(req);
        let resp = match apply_attempt_timeout(send_fut, effective_timeout).await {
            Ok(resp) => resp,
            Err(err) => {
                let reconnected = if should_reconnect(&err) {
                    // Atomically evicts failed_id and returns a new stream for use.
                    let new_stream = self.pool.evict_and_replace(stream_id);

                    // The application can `send()` multiple writes
                    // concurrently. Only one `send()` will update the cached
                    // stream on a transient error.
                    let _ = self.entry.compare_and_swap(&stream, Arc::new(new_stream));
                    true
                } else {
                    false
                };
                return (Err(err), stream_id, reconnected);
            }
        };

        let resp = match resp.cnv().map_err(Error::deser) {
            Ok(r) => r,
            Err(e) => return (Err(e.into()), stream_id, false),
        };
        (to_result(resp), stream_id, false)
    }
}

async fn apply_attempt_timeout<F, T>(fut: F, timeout: Option<Duration>) -> AppendResult<T>
where
    F: std::future::Future<Output = AppendResult<T>>,
{
    match timeout {
        Some(timeout) => match tokio::time::timeout(timeout, fut).await {
            Ok(res) => res,
            Err(_) => Err(AppendError::Rpc {
                source: Error::timeout("attempt timed out"),
            }),
        },
        None => fut.await,
    }
}

fn resolve_effective_timeout(
    attempt_timeout: Option<Duration>,
    remaining_time: Option<Duration>,
) -> Option<Duration> {
    match (attempt_timeout, remaining_time) {
        (None, None) => None,
        (None, Some(t)) => Some(t),
        (Some(t), None) => Some(t),
        (Some(a), Some(r)) => Some(std::cmp::min(a, r)),
    }
}

fn should_reconnect(err: &AppendError) -> bool {
    match err {
        AppendError::UnexpectedEndOfStream => true,
        AppendError::Rpc { source } => {
            source.is_transport()
                || source.is_io()
                || source.is_connect()
                || source.is_timeout()
                || source.status().is_some_and(|s| {
                    matches!(
                        s.code,
                        Code::Aborted | Code::Unavailable | Code::DeadlineExceeded
                    )
                })
        }
        AppendError::RowErrors(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::google::cloud::bigquery::storage::v1::AppendRowsResponse;
    use crate::google::cloud::bigquery::storage::v1::append_rows_response::Response;
    use crate::write::runner::WriteRequest;
    use crate::write::test::*;
    use bigquery_grpc_mock::{MockBigQueryWrite, start};
    use gaxi::grpc::tonic::{Response as TonicResponse, Status as TonicStatus};
    use google_cloud_gax::error::rpc::Status as GaxStatus;
    use google_cloud_gax::exponential_backoff::ExponentialBackoffBuilder;
    use google_cloud_gax::retry_policy::RetryPolicyExt;
    use http::HeaderMap;
    use std::error::Error as _;
    use std::time::Duration;
    use tokio::sync::{mpsc, oneshot};
    use tokio::task::JoinSet;

    fn new_test_dispatcher(pool: Arc<StreamPool>) -> Arc<Dispatcher> {
        Arc::new(Dispatcher::new(
            pool,
            test_retry_policy(),
            test_backoff_policy(),
            None,
        ))
    }

    fn test_req() -> AppendRowsRequest {
        AppendRowsRequest::new()
    }

    #[test]
    fn test_resolve_effective_timeout() {
        assert_eq!(resolve_effective_timeout(None, None), None);
        assert_eq!(
            resolve_effective_timeout(None, Some(Duration::from_secs(10))),
            Some(Duration::from_secs(10))
        );
        assert_eq!(
            resolve_effective_timeout(Some(Duration::from_secs(5)), None),
            Some(Duration::from_secs(5))
        );
        assert_eq!(
            resolve_effective_timeout(Some(Duration::from_secs(5)), Some(Duration::from_secs(10))),
            Some(Duration::from_secs(5))
        );
        assert_eq!(
            resolve_effective_timeout(Some(Duration::from_secs(10)), Some(Duration::from_secs(5))),
            Some(Duration::from_secs(5))
        );
    }

    #[test]
    fn should_reconnect_errors() {
        assert!(should_reconnect(&AppendError::UnexpectedEndOfStream));
        assert!(should_reconnect(
            &Error::transport(HeaderMap::default(), "transport").into()
        ));
        assert!(should_reconnect(&Error::io("io").into()));
        assert!(should_reconnect(&Error::connect("connect").into()));
        assert!(should_reconnect(&Error::timeout("timeout").into()));
        assert!(should_reconnect(
            &Error::service(GaxStatus::default().set_code(Code::Aborted)).into()
        ));
        assert!(should_reconnect(
            &Error::service(GaxStatus::default().set_code(Code::Unavailable)).into()
        ));
        assert!(should_reconnect(
            &Error::service(GaxStatus::default().set_code(Code::DeadlineExceeded)).into()
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
        let dispatcher = new_test_dispatcher(pool);
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
        let (response1_tx, response1_rx) = mpsc::channel(10);
        let (response2_tx, response2_rx) = mpsc::channel(10);
        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .times(1)
            .return_once(move |_| Ok(TonicResponse::from(response1_rx)));
        mock.expect_append_rows()
            .times(1)
            .return_once(move |_| Ok(TonicResponse::from(response2_rx)));

        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let transport = Arc::new(test_transport(endpoint).await?);
        let pool = Arc::new(StreamPool::new(transport, 10));

        let mut mock_backoff = MockBackoffPolicy::new();
        mock_backoff
            .expect_on_failure()
            .times(1)
            .return_const(Duration::ZERO);
        let dispatcher = Arc::new(Dispatcher::new(
            pool.clone(),
            test_retry_policy(),
            Arc::new(mock_backoff),
            None,
        ));
        assert_eq!(dispatcher.entry.load().id, 1);

        let write = {
            let d = dispatcher.clone();
            tokio::spawn(async move { d.send(test_req()).await })
        };

        // Simulate the stream closing before responding to the request.
        drop(response1_tx);

        // Stream 2 responds to the retried write.
        response2_tx.send(Ok(convert(&test_response(1)))).await?;

        let resp = write.await??;
        assert_eq!(resp.offset, Some(1));

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
        let dispatcher = new_test_dispatcher(pool.clone());
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
        let (response1_tx, response1_rx) = mpsc::channel(10);
        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .times(1)
            .return_once(move |_| Ok(TonicResponse::from(response1_rx)));
        mock.expect_append_rows()
            .times(1)
            .return_once(move |request| {
                let mut req_rx = request.into_inner();
                let (tx, rx) = mpsc::channel(1000);
                tokio::spawn(async move {
                    while req_rx.recv().await.is_some() {
                        let _ = tx.send(Ok(convert(&test_response(1)))).await;
                    }
                });
                Ok(TonicResponse::from(rx))
            });

        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let transport = Arc::new(test_transport(endpoint).await?);
        let pool = Arc::new(StreamPool::new(transport, 10));
        let dispatcher = Arc::new(Dispatcher::new(
            pool.clone(),
            Arc::new(google_cloud_gax::retry_policy::NeverRetry),
            test_backoff_policy(),
            None,
        ));
        assert_eq!(dispatcher.entry.load().id, 1);

        let mut writes = JoinSet::new();
        for _ in 0..1000 {
            let d = dispatcher.clone();
            writes.spawn(async move { d.send(test_req()).await });
        }

        // Simulate the stream closing before responding to the requests.
        drop(response1_tx);

        while let Some(write) = writes.join_next().await {
            assert!(write?.is_ok());
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
        let dispatcher = new_test_dispatcher(pool.clone());

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
        let dispatcher = new_test_dispatcher(pool.clone());
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
    async fn transport_error() -> anyhow::Result<()> {
        let (response2_tx, response2_rx) = mpsc::channel(10);
        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .times(1)
            .return_once(move |_| Ok(TonicResponse::from(response2_rx)));

        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let transport = Arc::new(test_transport(endpoint).await?);
        let pool = Arc::new(StreamPool::new(transport, 10));

        let mut mock_backoff = MockBackoffPolicy::new();
        mock_backoff
            .expect_on_failure()
            .times(1)
            .return_const(Duration::ZERO);
        let dispatcher = Arc::new(Dispatcher::new(
            pool.clone(),
            test_retry_policy(),
            Arc::new(mock_backoff),
            None,
        ));
        assert_eq!(dispatcher.entry.load().id, 1);

        let (stream1_tx, mut stream1_rx) = mpsc::unbounded_channel::<WriteRequest>();
        let mut entry = dispatcher.entry.load().as_ref().clone();
        entry.req_tx = stream1_tx;
        dispatcher.entry.store(Arc::new(entry));

        let write = {
            let d = dispatcher.clone();
            tokio::spawn(async move { d.send(test_req()).await })
        };

        // Stream 1 yields a transport error.
        let req1 = stream1_rx.recv().await.expect("request sent");
        let _ = req1.resp_tx.send(Err(
            Error::transport(HeaderMap::default(), "h2 reset").into()
        ));

        // Stream 2 yields Ok.
        response2_tx.send(Ok(convert(&test_response(1)))).await?;

        let resp = write.await??;
        assert_eq!(resp.offset, Some(1));

        assert_eq!(dispatcher.entry.load().id, 2);

        Ok(())
    }

    #[tokio::test]
    async fn server_restart() -> anyhow::Result<()> {
        let (response1_tx, response1_rx) = mpsc::channel(10);
        let (response2_tx, response2_rx) = mpsc::channel(10);
        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .times(1)
            .return_once(move |_| Ok(TonicResponse::from(response1_rx)));
        mock.expect_append_rows()
            .times(1)
            .return_once(move |_| Ok(TonicResponse::from(response2_rx)));

        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let transport = Arc::new(test_transport(endpoint).await?);
        let pool = Arc::new(StreamPool::new(transport, 10));

        let mut mock_backoff = MockBackoffPolicy::new();
        mock_backoff
            .expect_on_failure()
            .times(1)
            .return_const(Duration::ZERO);
        let dispatcher = Arc::new(Dispatcher::new(
            pool.clone(),
            test_retry_policy(),
            Arc::new(mock_backoff),
            None,
        ));
        assert_eq!(dispatcher.entry.load().id, 1);

        let write = {
            let d = dispatcher.clone();
            tokio::spawn(async move { d.send(test_req()).await })
        };

        // Stream 1 yields TonicStatus::aborted
        response1_tx
            .send(Err(TonicStatus::aborted(
                "Closing the stream because server is restarted",
            )))
            .await?;

        // Stream 2 yields Ok
        response2_tx.send(Ok(convert(&test_response(1)))).await?;

        let resp = write.await??;
        assert_eq!(resp.offset, Some(1));

        assert_eq!(dispatcher.entry.load().id, 2);

        Ok(())
    }

    #[tokio::test]
    async fn connect_error() -> anyhow::Result<()> {
        let (response2_tx, response2_rx) = mpsc::channel(10);
        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .times(1)
            .return_once(|_| Err(TonicStatus::unavailable("unavailable")));
        mock.expect_append_rows()
            .times(1)
            .return_once(move |_| Ok(TonicResponse::from(response2_rx)));

        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let transport = Arc::new(test_transport(endpoint).await?);
        let pool = Arc::new(StreamPool::new(transport, 10));

        let mut mock_backoff = MockBackoffPolicy::new();
        mock_backoff
            .expect_on_failure()
            .times(1)
            .return_const(Duration::ZERO);
        let dispatcher = Arc::new(Dispatcher::new(
            pool.clone(),
            test_retry_policy(),
            Arc::new(mock_backoff),
            None,
        ));
        assert_eq!(dispatcher.entry.load().id, 1);

        let write = {
            let d = dispatcher.clone();
            tokio::spawn(async move { d.send(test_req()).await })
        };

        // Stream 2 succeeds
        response2_tx.send(Ok(convert(&test_response(1)))).await?;

        let resp = write.await??;
        assert_eq!(resp.offset, Some(1));

        assert_eq!(dispatcher.entry.load().id, 2);

        Ok(())
    }

    #[tokio::test]
    async fn too_many_transients() -> anyhow::Result<()> {
        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .returning(|_| Err(TonicStatus::unavailable("try again")));

        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let transport = Arc::new(test_transport(endpoint).await?);
        let pool = Arc::new(StreamPool::new(transport, 10));

        let retry_policy =
            Arc::new(crate::write::retry_policy::RetryableErrors.with_attempt_limit(2));
        let dispatcher = Arc::new(Dispatcher::new(
            pool.clone(),
            retry_policy,
            test_backoff_policy(),
            None,
        ));
        assert_eq!(dispatcher.entry.load().id, 1);

        let err = dispatcher
            .send(test_req())
            .await
            .expect_err("should exhaust retries");
        let AppendError::Rpc { source } = err else {
            anyhow::bail!("expected AppendError::Rpc, got {err:?}");
        };
        assert!(source.is_exhausted(), "{source:?}");
        let inner = source
            .source()
            .and_then(|e| e.downcast_ref::<Error>())
            .expect("inner error");
        let status = inner.status().expect("status should be set");
        assert_eq!(status.code, Code::Unavailable);

        // Stream 1 and 2 both failed.
        assert_eq!(dispatcher.entry.load().id, 3);

        Ok(())
    }

    #[tokio::test]
    async fn library_retries_stream_closed_with_never_retry() -> anyhow::Result<()> {
        let (response1_tx, response1_rx) = mpsc::channel(10);
        let (response2_tx, response2_rx) = mpsc::channel(10);
        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .times(1)
            .return_once(move |_| Ok(TonicResponse::from(response1_rx)));
        mock.expect_append_rows()
            .times(1)
            .return_once(move |_| Ok(TonicResponse::from(response2_rx)));

        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let transport = Arc::new(test_transport(endpoint).await?);
        let pool = Arc::new(StreamPool::new(transport, 10));

        let dispatcher = Arc::new(Dispatcher::new(
            pool.clone(),
            Arc::new(google_cloud_gax::retry_policy::NeverRetry),
            test_backoff_policy(),
            None,
        ));
        assert_eq!(dispatcher.entry.load().id, 1);

        let write = {
            let d = dispatcher.clone();
            tokio::spawn(async move { d.send(test_req()).await })
        };

        // Stream 1 closes unexpectedly
        drop(response1_tx);

        // Stream 2 responds to the retried write
        response2_tx.send(Ok(convert(&test_response(1)))).await?;

        let resp = write.await??;
        assert_eq!(resp.offset, Some(1));
        assert_eq!(dispatcher.entry.load().id, 2);

        Ok(())
    }

    #[tokio::test]
    async fn attempt_timeout() -> anyhow::Result<()> {
        let mut mock = MockBigQueryWrite::new();
        // Stream 1 hangs (never sends a response)
        let (_tx1, response_rx1) = mpsc::channel(1);
        mock.expect_append_rows()
            .times(1)
            .return_once(move |_| Ok(TonicResponse::from(response_rx1)));
        // Stream 2 responds successfully
        let (response_tx2, response_rx2) = mpsc::channel(1);
        let expected = test_response(1);
        response_tx2.send(Ok(convert(&expected))).await?;
        mock.expect_append_rows()
            .times(1)
            .return_once(move |_| Ok(TonicResponse::from(response_rx2)));

        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let transport = Arc::new(test_transport(endpoint).await?);
        let pool = Arc::new(StreamPool::new(transport, 10));
        let dispatcher = Arc::new(Dispatcher::new(
            pool.clone(),
            test_retry_policy(),
            test_backoff_policy(),
            Some(Duration::from_millis(50)),
        ));
        assert_eq!(dispatcher.entry.load().id, 1);

        let resp = dispatcher.send(test_req()).await?;
        assert_eq!(resp.offset, Some(1));
        assert_eq!(dispatcher.entry.load().id, 2);

        Ok(())
    }

    #[tokio::test]
    async fn retry_exhausted() -> anyhow::Result<()> {
        let mut mock = MockBigQueryWrite::new();
        // Server hangs and never sends a response or closes the stream.
        let txs = Arc::new(std::sync::Mutex::new(Vec::new()));
        let txs_clone = txs.clone();
        mock.expect_append_rows().returning(move |_| {
            let (tx, rx) = mpsc::channel(10);
            txs_clone
                .lock()
                .expect("lock should not be poisoned")
                .push(tx);
            Ok(TonicResponse::from(rx))
        });

        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let transport = Arc::new(test_transport(endpoint).await?);
        let pool = Arc::new(StreamPool::new(transport, 10));
        let retry_policy = Arc::new(
            crate::write::retry_policy::RetryableErrors.with_time_limit(Duration::from_millis(80)),
        );
        let backoff_policy = Arc::new(
            ExponentialBackoffBuilder::default()
                .with_initial_delay(Duration::from_millis(10))
                .with_maximum_delay(Duration::from_millis(20))
                .with_scaling(2.0)
                .build()
                .expect("valid backoff configuration"),
        );
        let dispatcher = Arc::new(Dispatcher::new(
            pool,
            retry_policy,
            backoff_policy,
            Some(Duration::from_millis(30)),
        ));

        let err = dispatcher
            .send(test_req())
            .await
            .expect_err("should exhaust");
        let AppendError::Rpc { source } = err else {
            anyhow::bail!("expected AppendError::Rpc, got: {err:?}");
        };
        assert!(source.is_exhausted(), "{source:?}");

        Ok(())
    }

    #[tokio::test]
    async fn retry_exhausted_before_backoff_sleep() -> anyhow::Result<()> {
        let mut mock = MockBigQueryWrite::new();
        // Server immediately closes stream
        mock.expect_append_rows().times(1).return_once(|_| {
            let (tx, rx) = mpsc::channel(1);
            drop(tx);
            Ok(TonicResponse::from(rx))
        });

        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let transport = Arc::new(test_transport(endpoint).await?);
        let pool = Arc::new(StreamPool::new(transport, 10));
        let retry_policy = Arc::new(
            crate::write::retry_policy::RetryableErrors.with_time_limit(Duration::from_millis(20)),
        );
        // Backoff delay is much larger than time limit
        let backoff_policy = Arc::new(
            ExponentialBackoffBuilder::default()
                .with_initial_delay(Duration::from_secs(10))
                .with_maximum_delay(Duration::from_secs(20))
                .with_scaling(2.0)
                .build()
                .expect("valid backoff configuration"),
        );
        let dispatcher = Arc::new(Dispatcher::new(pool, retry_policy, backoff_policy, None));

        let start = std::time::Instant::now();
        let err = dispatcher
            .send(test_req())
            .await
            .expect_err("should exhaust without sleeping 10s");
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "should not sleep past deadline"
        );
        let AppendError::Rpc { source } = err else {
            anyhow::bail!("expected AppendError::Rpc, got: {err:?}");
        };
        assert!(source.is_exhausted(), "{source:?}");

        Ok(())
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
        let dispatcher = new_test_dispatcher(pool.clone());
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
