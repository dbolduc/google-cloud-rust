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

use super::runner::StreamWriterRunner;
use crate::google::cloud::bigquery::storage::v1::append_rows_request::{ArrowData, Rows};
use crate::google::cloud::bigquery::storage::v1::{
    AppendRowsRequest, AppendRowsResponse, ArrowRecordBatch, ArrowSchema,
};
use crate::transport::Transport;
use crate::{Error, Result};
use gaxi::grpc::from_status::to_gax_error;
use google_cloud_gax::error::rpc::{Code, Status};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::StreamExt;

/// A writer for a specific BigQuery write stream.
pub struct StreamWriter {
    request_tx: mpsc::Sender<(
        AppendRowsRequest,
        oneshot::Sender<Result<AppendRowsResponse>>,
    )>,
}

impl StreamWriter {
    pub(crate) fn new(transport: Arc<Transport>, stream_name: String, schema: ArrowSchema) -> Self {
        let (request_tx, request_rx) = mpsc::channel(100);
        tokio::spawn(async move {
            let mut runner = StreamWriterRunner::new(transport, stream_name, schema);
            let _ = runner.run(request_rx).await;
        });
        Self { request_tx }
    }

    /// Append Arrow record batches to the stream.
    pub async fn append(&self, rows: ArrowRecordBatch) -> Result<AppendRowsResponse> {
        let (tx, rx) = oneshot::channel();
        let request = AppendRowsRequest {
            rows: Some(Rows::ArrowRows(ArrowData {
                rows: Some(rows),
                ..Default::default()
            })),
            ..Default::default()
        };

        self.request_tx.send((request, tx)).await.map_err(|_| {
            Error::service(
                Status::default()
                    .set_code(Code::Cancelled)
                    .set_message("stream closed"),
            )
        })?;

        rx.await.map_err(|_| {
            Error::service(
                Status::default()
                    .set_code(Code::Cancelled)
                    .set_message("response channel closed"),
            )
        })?
    }
}
