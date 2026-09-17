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

use super::base::BaseWriter;
use super::writer::Format;
use crate::Result;
use crate::model::{FinalizeWriteStreamResponse, FlushRowsResponse};
use crate::write::builder::AppendWithOffset;
use crate::write::transport::Transport;
use std::sync::Arc;

/// A writer for a [buffered stream].
///
/// [buffered stream]: https://docs.cloud.google.com/bigquery/docs/write-api-grpc#buffered_type
#[derive(Debug)]
pub struct BufferedWriter<F> {
    pub(crate) inner: BaseWriter<F>,
}

impl<F> BufferedWriter<F> {
    pub(crate) fn new(inner: Arc<Transport>, write_stream: String, format: F) -> Self {
        Self {
            inner: BaseWriter::new(inner, write_stream, format),
        }
    }

    /// Return the full resource name of the underlying write stream.
    pub fn write_stream(&self) -> &str {
        &self.inner.write_stream
    }

    /// Flush the buffered stream, making rows up to the specified offset available for reading.
    pub async fn flush(&self, offset: i64) -> Result<FlushRowsResponse> {
        self.inner
            .client
            .flush_rows()
            .set_write_stream(&self.inner.write_stream)
            .set_offset(offset)
            .send()
            .await
    }

    /// Finalize the buffered stream, preventing further writes.
    pub async fn finalize(&self) -> Result<FinalizeWriteStreamResponse> {
        self.inner.finalize().await
    }
}

impl<F: Format> BufferedWriter<F> {
    /// Append rows to the buffered stream.
    pub fn append(&self, rows: F::Row) -> AppendWithOffset {
        AppendWithOffset::new(
            self.inner.runner.req_tx.clone(),
            self.inner.append_request(rows),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::writer::ArrowFormat;
    use super::*;
    use crate::error::AppendError;
    use crate::model::ArrowRecordBatch;
    use crate::write::test::*;
    use bigquery_grpc_mock::{MockBigQueryWrite, start};
    use gaxi::grpc::tonic::Response as TonicResponse;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn basic_success() -> anyhow::Result<()> {
        let (response_tx, response_rx) = mpsc::channel(10);

        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .return_once(|_| Ok(TonicResponse::from(response_rx)));

        mock.expect_flush_rows().return_once(|req| {
            assert_eq!(req.get_ref().offset, Some(3));
            assert_eq!(req.get_ref().write_stream, write_stream());
            Ok(TonicResponse::new(
                bigquery_grpc_mock::google::cloud::bigquery::storage::v1::FlushRowsResponse::default(),
            ))
        });

        mock.expect_finalize_write_stream().return_once(|req| {
            assert_eq!(req.get_ref().name, write_stream());
            Ok(TonicResponse::new(
                bigquery_grpc_mock::google::cloud::bigquery::storage::v1::FinalizeWriteStreamResponse::default(),
            ))
        });

        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let transport = Arc::new(test_transport(endpoint).await?);

        let writer = BufferedWriter::new(transport, write_stream(), ArrowFormat::new(schema()));
        assert_eq!(writer.write_stream(), write_stream());

        response_tx.send(Ok(convert(&test_response(1)))).await?;
        let resp = writer.append(arrow_rows(1)).send().await?;
        assert_eq!(resp.offset, Some(1));

        response_tx.send(Ok(convert(&test_response(2)))).await?;
        let resp = writer.append(arrow_rows(2)).send().await?;
        assert_eq!(resp.offset, Some(2));

        response_tx.send(Ok(convert(&test_response(3)))).await?;
        let resp = writer.append(arrow_rows(3)).send().await?;
        assert_eq!(resp.offset, Some(3));

        drop(response_tx);
        let err = writer
            .append(arrow_rows(4))
            .send()
            .await
            .expect_err("channel");
        assert!(matches!(err, AppendError::UnexpectedEndOfStream));

        writer.flush(3).await?;
        writer.finalize().await?;

        Ok(())
    }

    fn arrow_rows(id: i64) -> ArrowRecordBatch {
        ArrowRecordBatch::new().set_serialized_record_batch(id.to_string())
    }
}
