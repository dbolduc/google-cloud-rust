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

use super::super::builder::Append;
use super::super::dispatcher::Dispatcher;
use super::super::pool::StreamPool;
use crate::model::append_rows_request::ArrowData;
use crate::model::{AppendRowsRequest, ArrowRecordBatch, ArrowSchema};
use google_cloud_gax::backoff_policy::BackoffPolicy;
use google_cloud_gax::retry_policy::RetryPolicy;
use std::sync::Arc;
use std::time::Duration;

/// A writer for the [default stream]
///
/// [default stream]: https://docs.cloud.google.com/bigquery/docs/write-api#default_stream
#[derive(Debug)]
pub struct DefaultWriter {
    inner: Arc<Dispatcher>,
    pub(crate) write_stream: String,
    pub(crate) schema: ArrowSchema,
}

impl DefaultWriter {
    pub(crate) fn new(
        pool: Arc<StreamPool>,
        write_stream: String,
        schema: ArrowSchema,
        retry_policy: Arc<dyn RetryPolicy>,
        backoff_policy: Arc<dyn BackoffPolicy>,
        attempt_timeout: Option<Duration>,
    ) -> Self {
        let inner = Arc::new(Dispatcher::new(
            pool,
            retry_policy,
            backoff_policy,
            attempt_timeout,
        ));
        Self {
            inner,
            write_stream,
            schema,
        }
    }

    #[cfg(test)]
    pub(crate) fn pool(&self) -> &Arc<StreamPool> {
        &self.inner.pool
    }

    /// Append rows to the stream.
    pub fn append(&self, rows: ArrowRecordBatch) -> Append {
        // TODO(#5744) - send optimization
        let req = AppendRowsRequest::new()
            .set_write_stream(&self.write_stream)
            .set_arrow_rows(
                ArrowData::new()
                    .set_writer_schema(self.schema.clone())
                    .set_rows(rows),
            );
        Append::new(self.inner.clone(), req)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::write::test::*;
    use bigquery_grpc_mock::{MockBigQueryWrite, start};
    use gaxi::grpc::tonic::Response as TonicResponse;
    use tokio::sync::mpsc;

    fn test_writer(pool: Arc<StreamPool>) -> DefaultWriter {
        DefaultWriter::new(
            pool,
            write_stream(),
            schema(),
            test_retry_policy(),
            test_backoff_policy(),
            None,
        )
    }

    #[tokio::test]
    async fn request_fields() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("http://ignored:1").await?);
        let pool = Arc::new(StreamPool::new(transport, 1));
        let writer = test_writer(pool);

        let b = writer.append(rows(1));
        assert_eq!(b.req.write_stream, write_stream());
        let data = b.req.arrow_rows().expect("arrow rows should be set");
        let s = data.writer_schema.as_ref().expect("schema should be set");
        assert_eq!(s.serialized_schema, "test");
        let r = data.rows.as_ref().expect("rows should be set");
        assert_eq!(r.serialized_record_batch, "1");

        let b = writer.append(rows(2));
        assert_eq!(b.req.write_stream, write_stream());
        let data = b.req.arrow_rows().expect("arrow rows should be set");
        let s = data.writer_schema.as_ref().expect("schema should be set");
        assert_eq!(s.serialized_schema, "test");
        let r = data.rows.as_ref().expect("rows should be set");
        assert_eq!(r.serialized_record_batch, "2");

        Ok(())
    }

    #[tokio::test]
    async fn basic_success() -> anyhow::Result<()> {
        let (response1_tx, response1_rx) = mpsc::channel(10);
        let (response2_tx, response2_rx) = mpsc::channel(10);

        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .times(1)
            .return_once(|_| Ok(TonicResponse::from(response1_rx)));
        mock.expect_append_rows()
            .times(1)
            .return_once(|_| Ok(TonicResponse::from(response2_rx)));
        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let transport = Arc::new(test_transport(endpoint).await?);
        let pool = Arc::new(StreamPool::new(transport, 1));

        let writer = test_writer(pool);

        response1_tx.send(Ok(convert(&test_response(1)))).await?;
        let resp = writer.append(rows(1)).send().await?;
        assert_eq!(resp.offset, Some(1));

        response1_tx.send(Ok(convert(&test_response(2)))).await?;
        let resp = writer.append(rows(2)).send().await?;
        assert_eq!(resp.offset, Some(2));

        response1_tx.send(Ok(convert(&test_response(3)))).await?;
        let resp = writer.append(rows(3)).send().await?;
        assert_eq!(resp.offset, Some(3));

        drop(response1_tx);
        response2_tx.send(Ok(convert(&test_response(4)))).await?;
        let resp = writer.append(rows(4)).send().await?;
        assert_eq!(resp.offset, Some(4));

        Ok(())
    }

    fn rows(id: i64) -> ArrowRecordBatch {
        ArrowRecordBatch::new().set_serialized_record_batch(id.to_string())
    }
}
