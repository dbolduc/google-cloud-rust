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
use super::writer::{ArrowFormat, ProtoFormat};
use crate::Result;
use crate::model::{ArrowRecordBatch, FinalizeWriteStreamResponse, ProtoRows};
use crate::write::builder::AppendWithOffset;
use crate::write::transport::Transport;
use std::sync::Arc;

/// A writer for a [committed stream].
///
/// [committed stream]: https://docs.cloud.google.com/bigquery/docs/write-api-grpc#committed_type
#[derive(Debug)]
pub struct CommittedWriter<F> {
    pub(crate) inner: BaseWriter<F>,
}

impl<F> CommittedWriter<F> {
    pub(crate) fn new(inner: Arc<Transport>, write_stream: String, format: F) -> Self {
        Self {
            inner: BaseWriter::new(inner, write_stream, format),
        }
    }

    /// Return the full resource name of the underlying write stream.
    pub fn write_stream(&self) -> &str {
        &self.inner.write_stream
    }

    /// Finalize the stream, preventing further writes.
    pub async fn finalize(&self) -> Result<FinalizeWriteStreamResponse> {
        self.inner.finalize().await
    }
}

impl CommittedWriter<ArrowFormat> {
    /// Append rows to the committed stream.
    pub fn append(&self, rows: ArrowRecordBatch) -> AppendWithOffset {
        AppendWithOffset::new(
            self.inner.runner.req_tx.clone(),
            self.inner.append_request(rows),
        )
    }
}

impl CommittedWriter<ProtoFormat> {
    /// Append rows to the committed stream.
    pub fn append(&self, rows: ProtoRows) -> AppendWithOffset {
        AppendWithOffset::new(
            self.inner.runner.req_tx.clone(),
            self.inner.append_request(rows),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppendError;
    use crate::write::test::*;
    use bigquery_grpc_mock::{MockBigQueryWrite, start};
    use gaxi::grpc::tonic::Response as TonicResponse;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn arrow_request_fields() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("http://ignored:1").await?);
        let writer = CommittedWriter::new(transport, write_stream(), ArrowFormat::new(schema()));
        assert_eq!(writer.write_stream(), write_stream());

        let b = writer.append(arrow_rows(1));
        assert_eq!(b.req.write_stream, write_stream());
        let data = b.req.arrow_rows().expect("arrow rows should be set");
        let s = data.writer_schema.as_ref().expect("schema should be set");
        assert_eq!(s.serialized_schema, "test");
        let r = data.rows.as_ref().expect("rows should be set");
        assert_eq!(r.serialized_record_batch, "1");

        Ok(())
    }

    #[tokio::test]
    async fn arrow_basic_success() -> anyhow::Result<()> {
        let (response_tx, response_rx) = mpsc::channel(10);

        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows()
            .return_once(|_| Ok(TonicResponse::from(response_rx)));

        mock.expect_finalize_write_stream().return_once(|_| {
            Ok(TonicResponse::new(
                bigquery_grpc_mock::google::cloud::bigquery::storage::v1::FinalizeWriteStreamResponse::default(),
            ))
        });

        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;
        let transport = Arc::new(test_transport(endpoint).await?);

        let writer = CommittedWriter::new(transport, write_stream(), ArrowFormat::new(schema()));
        assert_eq!(writer.write_stream(), write_stream());

        response_tx.send(Ok(convert(&test_response(1)))).await?;
        let resp = writer.append(arrow_rows(1)).send().await?;
        assert_eq!(resp.offset, Some(1));

        drop(response_tx);
        let err = writer
            .append(arrow_rows(2))
            .send()
            .await
            .expect_err("channel");
        assert!(matches!(err, AppendError::UnexpectedEndOfStream));

        writer.finalize().await?;

        Ok(())
    }

    #[tokio::test]
    async fn proto_request_fields() -> anyhow::Result<()> {
        let transport = Arc::new(test_transport("http://ignored:1").await?);
        let writer =
            CommittedWriter::new(transport, write_stream(), ProtoFormat::new(proto_schema()));
        assert_eq!(writer.write_stream(), write_stream());

        let b = writer.append(proto_rows(1));
        assert_eq!(b.req.write_stream, write_stream());
        let data = b.req.proto_rows().expect("proto rows should be set");
        let s = data.writer_schema.as_ref().expect("schema should be set");
        assert_eq!(s.proto_descriptor.as_ref().unwrap().name, "TestMessage");
        let r = data.rows.as_ref().expect("rows should be set");
        assert_eq!(r.serialized_rows, vec![bytes::Bytes::from("1")]);

        Ok(())
    }

    fn arrow_rows(id: i64) -> ArrowRecordBatch {
        ArrowRecordBatch::new().set_serialized_record_batch(id.to_string())
    }

    fn proto_rows(id: i64) -> ProtoRows {
        ProtoRows::new().set_serialized_rows(vec![bytes::Bytes::from(id.to_string())])
    }
}
