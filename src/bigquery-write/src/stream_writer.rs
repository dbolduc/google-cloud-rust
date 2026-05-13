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

struct StreamWriterRunner {
    transport: Arc<Transport>,
    stream_name: String,
    schema: ArrowSchema,
    pending_responses: VecDeque<oneshot::Sender<Result<AppendRowsResponse>>>,
}

impl StreamWriterRunner {
    fn new(transport: Arc<Transport>, stream_name: String, schema: ArrowSchema) -> Self {
        Self {
            transport,
            stream_name,
            schema,
            pending_responses: VecDeque::new(),
        }
    }

    async fn run(
        &mut self,
        mut request_rx: mpsc::Receiver<(
            AppendRowsRequest,
            oneshot::Sender<Result<AppendRowsResponse>>,
        )>,
    ) -> Result<()> {
        let (mut first_request, first_response_tx) = match request_rx.recv().await {
            Some(r) => r,
            None => return Ok(()),
        };

        let (grpc_request_tx, grpc_request_rx) = mpsc::channel(100);
        let request_params = format!("write_stream={}", self.stream_name);

        // Prepare the first request
        first_request.write_stream = self.stream_name.clone();
        if let Some(Rows::ArrowRows(ref mut arrow_data)) = first_request.rows {
            arrow_data.writer_schema = Some(self.schema.clone());
        }

        // Send the first request to the gRPC stream channel before opening the stream
        // to avoid deadlock with servers that wait for the first request.
        grpc_request_tx.send(first_request).await.map_err(|_| {
            Error::service(
                Status::default()
                    .set_code(Code::Internal)
                    .set_message("internal channel closed"),
            )
        })?;
        self.pending_responses.push_back(first_response_tx);

        tracing::info!("Opening AppendRows stream for {}", self.stream_name);
        let mut grpc_response_stream = self
            .transport
            .append_rows(
                &request_params,
                grpc_request_rx,
                crate::RequestOptions::default(),
            )
            .await?
            .into_inner();

        loop {
            tokio::select! {
                Some((mut request, response_tx)) = request_rx.recv() => {
                    tracing::debug!("Received request for append");
                    request.write_stream = self.stream_name.clone();
                    self.pending_responses.push_back(response_tx);
                    if grpc_request_tx.send(request).await.is_err() {
                        tracing::error!("Failed to send request to gRPC stream");
                        break;
                    }
                }
                Some(response) = grpc_response_stream.next() => {
                    tracing::debug!("Received response from gRPC stream");
                    if let Some(pending) = self.pending_responses.pop_front() {
                        let _ = pending.send(response.map_err(to_gax_error));
                    }
                }
                else => {
                    tracing::info!("Stream closed");
                    break;
                }
            }
        }

        // Notify remaining pending requests that the stream has closed
        while let Some(pending) = self.pending_responses.pop_front() {
            let _ = pending.send(Err(Error::service(
                Status::default()
                    .set_code(Code::Cancelled)
                    .set_message("stream closed"),
            )));
        }

        Ok(())
    }
}
