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

use crate::append_result::AppendResult;
use crate::error::AppendError;
use crate::google::cloud::bigquery::storage::v1;
use crate::model::append_rows_request::ArrowData;
use crate::model::{AppendRowsRequest, AppendRowsResponse, ArrowRecordBatch, ArrowSchema};
use crate::runner::{Runner, WriteRequest};
use crate::transport::Transport;
use gaxi::prost::{FromProto, ToProto};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

pub struct PendingWriter {
    runner: Runner,
    write_stream: String,
    schema: ArrowSchema,
}

impl PendingWriter {
    pub(crate) fn new(inner: Arc<Transport>, write_stream: String, schema: ArrowSchema) -> Self {
        let runner = Runner::new(inner);
        Self {
            runner,
            write_stream,
            schema,
        }
    }

    pub fn append(&self, rows: ArrowRecordBatch) -> AppendBuilder {
        let req = AppendRowsRequest::new()
            .set_write_stream(&self.write_stream)
            .set_arrow_rows(
                ArrowData::new()
                    .set_writer_schema(self.schema.clone())
                    .set_rows(rows),
            );
        AppendBuilder::new(self.runner.req_tx.clone(), req)
    }
}

pub struct AppendBuilder {
    req_tx: mpsc::Sender<WriteRequest>,
    // TODO : send optimization. We will want refs and won't want to store this in a req.
    req: AppendRowsRequest,
}

impl AppendBuilder {
    fn new(req_tx: mpsc::Sender<WriteRequest>, req: AppendRowsRequest) -> Self {
        Self { req_tx, req }
    }

    pub fn set_offset(mut self, offset: i64) -> Self {
        self.req = self.req.set_offset(offset);
        self
    }

    pub async fn send(self) -> crate::error::AppendResult<AppendResult> {
        let (resp_tx, resp_rx) = oneshot::channel();
        // TODO : wrong error type.
        //        Do we want like an internal error thing?
        //        It probably can't fail in practice.
        let req = self
            .req
            .to_proto()
            .map_err(|_| AppendError::UnexpectedEndOfStream)?;
        let write = WriteRequest { req, resp_tx };
        let _ = self.req_tx.send(write).await;
        let resp = resp_rx
            .await
            .map_err(|_| AppendError::UnexpectedEndOfStream)??;
        // TODO : no need to convert the whole struct.
        let resp = resp.cnv().map_err(|_| AppendError::UnexpectedEndOfStream)?;
        to_append_result(resp)
    }
}

fn to_append_result(resp: AppendRowsResponse) -> crate::error::AppendResult<AppendResult> {
    // TODO : map stream_errors.
    Ok(AppendResult {
        updated_schema: resp.updated_schema,
    })
}
