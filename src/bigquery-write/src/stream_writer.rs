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

use super::runner::{StreamWriterRunner, WriterSchema};
use crate::google::cloud::bigquery::storage::v1::append_rows_request::{ArrowData, ProtoData, Rows};
use crate::google::cloud::bigquery::storage::v1::{
    AppendRowsRequest, AppendRowsResponse, ArrowRecordBatch, ArrowSchema, ProtoRows, ProtoSchema,
};
use crate::transport::Transport;
use crate::{Error, Result};
use google_cloud_gax::error::rpc::{Code, Status};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

/// A writer for a specific BigQuery write stream using Arrow format.
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
            let mut runner =
                StreamWriterRunner::new(transport, stream_name, WriterSchema::Arrow(schema));
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

        self.send_request(request, tx).await?;
        self.recv_response(rx).await
    }

    pub(crate) async fn send_request(
        &self,
        request: AppendRowsRequest,
        tx: oneshot::Sender<Result<AppendRowsResponse>>,
    ) -> Result<()> {
        self.request_tx.send((request, tx)).await.map_err(|_| {
            Error::service(
                Status::default()
                    .set_code(Code::Cancelled)
                    .set_message("stream closed"),
            )
        })
    }

    pub(crate) async fn recv_response(
        &self,
        rx: oneshot::Receiver<Result<AppendRowsResponse>>,
    ) -> Result<AppendRowsResponse> {
        rx.await.map_err(|_| {
            Error::service(
                Status::default()
                    .set_code(Code::Cancelled)
                    .set_message("response channel closed"),
            )
        })?
    }
}

/// A writer for a specific BigQuery write stream using Proto format.
pub struct ProtoStreamWriter {
    request_tx: mpsc::Sender<(
        AppendRowsRequest,
        oneshot::Sender<Result<AppendRowsResponse>>,
    )>,
}

impl ProtoStreamWriter {
    pub(crate) fn new(transport: Arc<Transport>, stream_name: String, schema: ProtoSchema) -> Self {
        let (request_tx, request_rx) = mpsc::channel(100);
        tokio::spawn(async move {
            let mut runner =
                StreamWriterRunner::new(transport, stream_name, WriterSchema::Proto(schema));
            let _ = runner.run(request_rx).await;
        });
        Self { request_tx }
    }

    /// Append Proto rows to the stream.
    pub async fn append(&self, rows: ProtoRows) -> Result<AppendRowsResponse> {
        let (tx, rx) = oneshot::channel();
        let request = AppendRowsRequest {
            rows: Some(Rows::ProtoRows(ProtoData {
                rows: Some(rows),
                ..Default::default()
            })),
            ..Default::default()
        };

        self.send_request(request, tx).await?;
        self.recv_response(rx).await
    }

    pub(crate) async fn send_request(
        &self,
        request: AppendRowsRequest,
        tx: oneshot::Sender<Result<AppendRowsResponse>>,
    ) -> Result<()> {
        self.request_tx.send((request, tx)).await.map_err(|_| {
            Error::service(
                Status::default()
                    .set_code(Code::Cancelled)
                    .set_message("stream closed"),
            )
        })
    }

    pub(crate) async fn recv_response(
        &self,
        rx: oneshot::Receiver<Result<AppendRowsResponse>>,
    ) -> Result<AppendRowsResponse> {
        rx.await.map_err(|_| {
            Error::service(
                Status::default()
                    .set_code(Code::Cancelled)
                    .set_message("response channel closed"),
            )
        })?
    }
}
