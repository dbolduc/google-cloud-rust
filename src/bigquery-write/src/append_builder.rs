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
use crate::dispatcher::Dispatcher;
use crate::error::AppendResult;
use crate::model::{AppendResponse, AppendRowsRequest};
use gaxi::prost::{FromProto, ToProto};
use std::sync::Arc;

/// A request builder for appending rows on the default stream.
#[derive(Clone, Debug)]
pub struct Append {
    inner: Arc<Dispatcher>,
    pub(crate) req: AppendRowsRequest,
}

impl Append {
    pub(crate) fn new(inner: Arc<Dispatcher>, req: AppendRowsRequest) -> Self {
        Self { inner, req }
    }

    /// Append rows to the stream.
    pub async fn send(self) -> AppendResult<AppendResponse> {
        let req_proto = self.req.to_proto().map_err(Error::deser)?;
        let resp_proto = self.inner.send_request(req_proto).await?;
        let resp = resp_proto.cnv().map_err(Error::ser)?;
        to_result(resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppendError;
    use crate::google::cloud::bigquery::storage::v1;
    use crate::google::cloud::bigquery::storage::v1::append_rows_response::{
        AppendResult, Response,
    };
    use crate::model::TableSchema;
    use crate::pool::{DEFAULT_MAX_POOL_SIZE, StreamEntry, StreamPool};
    use crate::runner::WriteRequest;
    use crate::transport::tests::test_transport;
    use arc_swap::ArcSwap;
    use tokio::sync::mpsc;

    async fn test_shared_state(req_tx: mpsc::Sender<WriteRequest>) -> Arc<Dispatcher> {
        let transport = Arc::new(
            test_transport("http://ignored:1".to_string())
                .await
                .unwrap(),
        );
        let pool = Arc::new(StreamPool::new(transport, DEFAULT_MAX_POOL_SIZE));
        let stream = StreamEntry {
            id: 1,
            sender: req_tx,
            outstanding_requests: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            outstanding_bytes: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        };
        Arc::new(Dispatcher::new(pool, ArcSwap::from_pointee(stream)))
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
