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

use crate::google::cloud::bigquery::storage::v1::AppendRowsRequest;
use crate::google::cloud::bigquery::storage::v1::append_rows_request::{
    ArrowData, ProtoData, Rows,
};
use crate::model::{AppendRowsResponse, ArrowRecordBatch, ArrowSchema, ProtoRows};
use crate::pool::{AppendRequest, StreamCommand, StreamHandle};
use crate::proto_schema::ProtoSchema;
use crate::{Error, Result};
use gaxi::prost::{FromProto, ToProto};
use google_cloud_gax::error::rpc::{Code, Status};
use tokio::sync::oneshot;

/// A writer for a specific BigQuery write stream using Arrow format.
pub struct ArrowStreamWriter {
    handle: StreamHandle,
    stream_name: String,
    schema: ArrowSchema,
}

impl ArrowStreamWriter {
    pub(crate) fn new(handle: StreamHandle, stream_name: String, schema: ArrowSchema) -> Self {
        Self {
            handle,
            stream_name,
            schema,
        }
    }

    /// Append Arrow record batches to the stream.
    pub async fn append(&self, rows: ArrowRecordBatch) -> Result<AppendRowsResponse> {
        let (tx, rx) = oneshot::channel();
        let request = AppendRowsRequest {
            write_stream: self.stream_name.clone(),
            rows: Some(Rows::ArrowRows(ArrowData {
                rows: Some(rows.to_proto().map_err(Error::ser)?),
                writer_schema: Some(self.schema.clone().to_proto().map_err(Error::ser)?),
            })),
            ..Default::default()
        };

        self.handle
            .tx
            .send(StreamCommand::Append(AppendRequest {
                request,
                resp_tx: tx,
            }))
            .await
            .map_err(|_| {
                Error::service(
                    Status::default()
                        .set_code(Code::Cancelled)
                        .set_message("stream closed"),
                )
            })?;

        rx.await
            .map_err(|_| {
                Error::service(
                    Status::default()
                        .set_code(Code::Cancelled)
                        .set_message("response channel closed"),
                )
            })??
            .cnv()
            .map_err(Error::ser)
    }
}

/// A writer for a specific BigQuery write stream using Proto format.
pub struct ProtoStreamWriter {
    handle: StreamHandle,
    stream_name: String,
    schema: ProtoSchema,
}

impl ProtoStreamWriter {
    pub(crate) fn new(handle: StreamHandle, stream_name: String, schema: ProtoSchema) -> Self {
        Self {
            handle,
            stream_name,
            schema,
        }
    }

    /// Append Proto rows to the stream.
    pub async fn append(&self, rows: ProtoRows) -> Result<AppendRowsResponse> {
        let (tx, rx) = oneshot::channel();
        let request = AppendRowsRequest {
            write_stream: self.stream_name.clone(),
            rows: Some(Rows::ProtoRows(ProtoData {
                rows: Some(rows.to_proto().map_err(Error::ser)?),
                writer_schema: Some(
                    ToProto::<crate::google::cloud::bigquery::storage::v1::ProtoSchema>::to_proto(
                        self.schema.clone(),
                    )
                    .map_err(Error::ser)?,
                ),
            })),
            ..Default::default()
        };

        self.handle
            .tx
            .send(StreamCommand::Append(AppendRequest {
                request,
                resp_tx: tx,
            }))
            .await
            .map_err(|_| {
                Error::service(
                    Status::default()
                        .set_code(Code::Cancelled)
                        .set_message("stream closed"),
                )
            })?;

        rx.await
            .map_err(|_| {
                Error::service(
                    Status::default()
                        .set_code(Code::Cancelled)
                        .set_message("response channel closed"),
                )
            })??
            .cnv()
            .map_err(Error::ser)
    }
}
