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

use crate::append_response::to_result;
use crate::builder::write::AppendWithOffset;
use crate::error::{AppendError, AppendResult};
use crate::google::cloud::bigquery::storage::v1;
use crate::model::AppendResponse;
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

    pub fn append(&self, rows: ArrowRecordBatch) -> AppendWithOffset {
        let req = AppendRowsRequest::new()
            .set_write_stream(&self.write_stream)
            .set_arrow_rows(
                ArrowData::new()
                    .set_writer_schema(self.schema.clone())
                    .set_rows(rows),
            );
        AppendWithOffset::new(self.runner.req_tx.clone(), req)
    }
}
