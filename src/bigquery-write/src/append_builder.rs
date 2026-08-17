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

use crate::Error;
use crate::append_response::to_result;
use crate::arrow::WriterSharedState;
use crate::error::{AppendError, AppendResult};
use crate::model::{AppendResponse, AppendRowsRequest};
use crate::runner::WriteRequest;
use gaxi::prost::{FromProto, ToProto};
use prost::Message;
use std::sync::Arc;
use tokio::sync::oneshot;

/// A request builder for appending rows on the default stream.
#[derive(Clone, Debug)]
pub struct Append {
    inner: Arc<WriterSharedState>,
    pub(crate) req: AppendRowsRequest,
}

impl Append {
    pub(crate) fn new(inner: Arc<WriterSharedState>, req: AppendRowsRequest) -> Self {
        Self { inner, req }
    }

    /// Append rows to the stream.
    pub async fn send(self) -> AppendResult<AppendResponse> {
        let stream = self.inner.cached_stream.load_full();
        let stream_id = stream.id;
        let sender = stream.sender.clone();
        let outstanding_requests = stream.outstanding_requests.clone();
        let outstanding_bytes = stream.outstanding_bytes.clone();

        let (resp_tx, resp_rx) = oneshot::channel();
        let req_proto = self.req.to_proto().map_err(Error::deser)?;
        let req_len = req_proto.encoded_len();

        let write = WriteRequest {
            req: req_proto,
            resp_tx,
        };

        // Increment load metrics.
        outstanding_requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        outstanding_bytes.fetch_add(req_len as u64, std::sync::atomic::Ordering::Relaxed);

        match sender.send(write).await {
            Ok(()) => {
                match resp_rx.await {
                    Ok(Ok(resp)) => {
                        // Decrement metrics on success.
                        outstanding_requests.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                        outstanding_bytes
                            .fetch_sub(req_len as u64, std::sync::atomic::Ordering::Relaxed);

                        let resp = resp.cnv().map_err(Error::ser)?;
                        to_result(resp)
                    }
                    Ok(Err(err)) => {
                        // Decrement metrics on business-logic errors.
                        outstanding_requests.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                        outstanding_bytes
                            .fetch_sub(req_len as u64, std::sync::atomic::Ordering::Relaxed);
                        Err(err)
                    }
                    Err(_) => {
                        // Decrement metrics on background connection crash.
                        outstanding_requests.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                        outstanding_bytes
                            .fetch_sub(req_len as u64, std::sync::atomic::Ordering::Relaxed);

                        // Stream connection died mid-stream! Evict & re-bind.
                        self.inner.pool.evict_and_replace(stream_id);
                        let new_stream = self.inner.pool.get_least_loaded_stream();
                        self.inner.cached_stream.store(Arc::new(new_stream));

                        // TODO: Implement retry loop here in a future PR.
                        Err(AppendError::UnexpectedEndOfStream)
                    }
                }
            }
            Err(_) => {
                // Decrement metrics as send failed.
                outstanding_requests.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                outstanding_bytes.fetch_sub(req_len as u64, std::sync::atomic::Ordering::Relaxed);

                // Stream connection closed/dead! Evict & re-bind.
                self.inner.pool.evict_and_replace(stream_id);
                let new_stream = self.inner.pool.get_least_loaded_stream();
                self.inner.cached_stream.store(Arc::new(new_stream));

                // TODO: Implement retry loop here in a future PR.
                Err(AppendError::UnexpectedEndOfStream)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::google::cloud::bigquery::storage::v1;
    use crate::google::cloud::bigquery::storage::v1::append_rows_response::{
        AppendResult, Response,
    };
    use crate::model::TableSchema;
    use crate::pool::{StreamEntry, StreamPool};
    use crate::transport::tests::test_transport;
    use arc_swap::ArcSwap;
    use tokio::sync::mpsc;

    async fn test_shared_state(req_tx: mpsc::Sender<WriteRequest>) -> Arc<WriterSharedState> {
        let transport = Arc::new(
            test_transport("http://ignored:1".to_string())
                .await
                .unwrap(),
        );
        let pool = Arc::new(StreamPool::new(transport, 1));
        let stream = StreamEntry {
            id: 1,
            sender: req_tx,
            outstanding_requests: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            outstanding_bytes: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        };
        Arc::new(WriterSharedState {
            pool,
            cached_stream: ArcSwap::from_pointee(stream),
        })
    }

    #[tokio::test]
    async fn success() -> anyhow::Result<()> {
        let (req_tx, mut req_rx) = mpsc::channel(10);
        let inner = test_shared_state(req_tx).await;
        let req = AppendRowsRequest::new().set_write_stream(write_stream());

        let builder = Append::new(inner, req);
        let handle = tokio::spawn(async move { builder.send().await });

        // Receive and verify the request
        let write = req_rx.recv().await.expect("should receive request");
        assert_eq!(write.req.write_stream, write_stream());

        // Provide a successful response
        let resp = v1::AppendRowsResponse {
            response: Some(Response::AppendResult(AppendResult::default())),
            write_stream: write_stream(),
            updated_schema: Some(v1::TableSchema::default()),
            ..Default::default()
        };
        write
            .resp_tx
            .send(Ok(resp))
            .expect("sending on channel always succeeds");

        let resp = handle.await??;
        assert_eq!(resp.offset, None);
        assert_eq!(resp.updated_schema, Some(TableSchema::default()));
        Ok(())
    }

    #[tokio::test]
    async fn stream_closed() -> anyhow::Result<()> {
        let (req_tx, req_rx) = mpsc::channel(10);
        let inner = test_shared_state(req_tx).await;
        let req = AppendRowsRequest::new().set_write_stream(write_stream());

        let builder = Append::new(inner, req);
        let handle = tokio::spawn(async move { builder.send().await });

        // Simulate a stream closure
        drop(req_rx);

        let err = handle.await?.expect_err("should return an error");
        assert!(matches!(err, AppendError::UnexpectedEndOfStream));
        Ok(())
    }

    #[tokio::test]
    async fn rpc_error() -> anyhow::Result<()> {
        let (req_tx, mut req_rx) = mpsc::channel(10);
        let inner = test_shared_state(req_tx).await;
        let req = AppendRowsRequest::new().set_write_stream(write_stream());

        let builder = Append::new(inner, req);
        let handle = tokio::spawn(async move { builder.send().await });

        // Simulate a stream ending in a known error
        let write = req_rx.recv().await.expect("should receive request");
        let append_err: AppendError = crate::Error::io("fail").into();
        write
            .resp_tx
            .send(Err(append_err))
            .expect("sending on channel always succeeds");

        let err = handle.await?.expect_err("should return an error");
        assert!(matches!(err, AppendError::Rpc { source: _ }));
        Ok(())
    }

    #[tokio::test]
    async fn row_errors() -> anyhow::Result<()> {
        let (req_tx, mut req_rx) = mpsc::channel(10);
        let inner = test_shared_state(req_tx).await;
        let req = AppendRowsRequest::new().set_write_stream(write_stream());

        let builder = Append::new(inner, req);
        let handle = tokio::spawn(async move { builder.send().await });

        let write = req_rx.recv().await.expect("should receive request");

        let row_error = v1::RowError {
            index: 42,
            code: v1::row_error::RowErrorCode::FieldsError as i32,
            message: "fail".to_string(),
        };
        let resp = v1::AppendRowsResponse {
            row_errors: vec![row_error],
            write_stream: write_stream(),
            ..Default::default()
        };
        write
            .resp_tx
            .send(Ok(resp))
            .expect("sending on channel always succeeds");

        let err = handle.await?.expect_err("should return an error");
        assert!(matches!(err, AppendError::RowErrors(_)));
        Ok(())
    }

    fn write_stream() -> String {
        "projects/p/datasets/d/tables/t/streams/_default".to_string()
    }
}
