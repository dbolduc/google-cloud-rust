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

pub struct Append {
    req_tx: mpsc::Sender<WriteRequest>,
    // TODO : send optimization. We will want refs and won't want to store this in a req.
    req: AppendRowsRequest,
}

impl Append {
    pub(crate) fn new(req_tx: mpsc::Sender<WriteRequest>, req: AppendRowsRequest) -> Self {
        Self { req_tx, req }
    }

    pub async fn send(self) -> AppendResult<AppendResponse> {
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
        to_result(resp)
    }
}

pub struct AppendWithOffset {
    req_tx: mpsc::Sender<WriteRequest>,
    // TODO : send optimization. We will want refs and won't want to store this in a req.
    req: AppendRowsRequest,
}

impl AppendWithOffset {
    pub(crate) fn new(req_tx: mpsc::Sender<WriteRequest>, req: AppendRowsRequest) -> Self {
        Self { req_tx, req }
    }

    pub fn set_offset(mut self, offset: i64) -> Self {
        self.req = self.req.set_offset(offset);
        self
    }

    pub async fn send(self) -> AppendResult<AppendResponse> {
        let (resp_tx, resp_rx) = oneshot::channel();
        // TODO : wrong error type.
        //        Do we want like an internal error thing?
        //        It probably can't fail in practice.
        let req = self
            .req
            .to_proto()
            .map_err(|_| AppendError::UnexpectedEndOfStream)?;
        let write = WriteRequest { req, resp_tx };
        // TODO : probably ought to be an unbounded sender for use in a sync fn.
        // `blocking_send()` seems wrong to me.
        let _ = self.req_tx.send(write).await;
        let resp = resp_rx
            .await
            .map_err(|_| AppendError::UnexpectedEndOfStream)??;
        // TODO : no need to convert the whole struct.
        let resp = resp.cnv().map_err(|_| AppendError::UnexpectedEndOfStream)?;
        to_result(resp)
    }
}
