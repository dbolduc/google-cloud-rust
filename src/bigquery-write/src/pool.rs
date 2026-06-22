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

use crate::google::cloud::bigquery::storage::v1::{
    AppendRowsRequest, AppendRowsResponse, ArrowRecordBatch, ArrowSchema,
};
use crate::{Error, Result};

/// An abstraction
pub(super) struct Stream {}

/// The client's connection pool.
///
/// This pool is scoped to a region, e.g. "us-east1".
///
/// It holds handles for `StreamActors`, for each bidirectional stream.
///
/// ## Default stream
///
/// At a high level, the default stream can be multiplexed across many tables.
/// Requests to the default stream are serviced by N `StreamActors`.
///
/// The connection pool grows dynamically. If there is no available stream, we
/// add new streams (up to a limit). We may rebalance this pool dynamically too.
///
/// ## Custom streams (buffered, committed, pending)
///
/// Any custom streams have their own `StreamActor`. These are added to the pool
/// and removed from the pool as needed.
pub(super) struct ConnectionPool {}

struct StreamRequest {
    rows: ArrowRecordBatch,
    resp_tx: tokio::sync::oneshot::Sender<Result<AppendRowsResponse>>,
}

/// A handle to a task running a stream
struct StreamActor {
    tx: tokio::sync::mpsc::Sender<StreamRequest>,
}

impl ConnectionPool {
    pub async fn append(&self, rows: ArrowRecordBatch) -> Result<AppendRowsResponse> {
        // TODO : retry loop.

        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();

        // Routing
        let handle = self.lookup_stream();
        let stream_req = StreamRequest { rows, resp_tx };
        handle.tx.send(stream_req).await.map_err(Error::io)?;

        resp_rx.await.map_err(Error::io)?
    }

    fn lookup_stream(&self) -> StreamActor {
        todo!();
    }
}
