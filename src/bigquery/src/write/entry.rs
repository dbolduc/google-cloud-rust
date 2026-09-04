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

use super::error::{AppendError, AppendResult};
use super::runner::{Runner, WriteRequest};
use super::transport::Transport;
use prost::Message;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};

/// An entry representing an active, healthy stream connection (Runner).
#[derive(Clone, Debug)]
pub(crate) struct StreamEntry {
    /// Unique identifier for this stream connection.
    pub(crate) id: u64,
    /// Channel to send requests to the stream's background runner task.
    pub(crate) req_tx: mpsc::UnboundedSender<WriteRequest>,
    /// Track the number of outstanding requests on this stream.
    pub(crate) outstanding_requests: Arc<AtomicU64>,
    /// Track the total outstanding bytes on this stream.
    pub(crate) outstanding_bytes: Arc<AtomicU64>,
}

impl StreamEntry {
    /// Dispatches a single append request directly over the runner channel.
    pub(crate) async fn send(
        &self,
        req: crate::google::cloud::bigquery::storage::v1::AppendRowsRequest,
    ) -> AppendResult<crate::google::cloud::bigquery::storage::v1::AppendRowsResponse> {
        let req_len = req.encoded_len() as u64;
        let _guard = LoadGuard::new(self, req_len);

        let (resp_tx, resp_rx) = oneshot::channel();
        let write = WriteRequest { req, resp_tx };

        self.req_tx
            .send(write)
            .map_err(|_| AppendError::UnexpectedEndOfStream)?;

        match resp_rx.await {
            Ok(Ok(resp)) => Ok(resp),
            Ok(Err(err)) => Err(err),
            Err(_) => Err(AppendError::UnexpectedEndOfStream),
        }
    }
}

/// RAII guard that increments load metrics on entry and decrements on drop.
pub(crate) struct LoadGuard {
    outstanding_requests: Arc<AtomicU64>,
    outstanding_bytes: Arc<AtomicU64>,
    bytes: u64,
}

impl LoadGuard {
    pub(crate) fn new(entry: &StreamEntry, bytes: u64) -> Self {
        entry.outstanding_requests.fetch_add(1, Ordering::Relaxed);
        entry.outstanding_bytes.fetch_add(bytes, Ordering::Relaxed);
        Self {
            outstanding_requests: Arc::clone(&entry.outstanding_requests),
            outstanding_bytes: Arc::clone(&entry.outstanding_bytes),
            bytes,
        }
    }
}

impl Drop for LoadGuard {
    fn drop(&mut self) {
        self.outstanding_requests.fetch_sub(1, Ordering::Relaxed);
        self.outstanding_bytes
            .fetch_sub(self.bytes, Ordering::Relaxed);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::runner::tests::*;
    use super::super::transport::tests::*;
    use super::*;
    use bigquery_grpc_mock::{MockBigQueryWrite, start};
    use gaxi::grpc::tonic::Response as TonicResponse;
    use tokio::sync::oneshot;
    use tokio::task::JoinSet;

    #[tokio::test]
    async fn todo() -> anyhow::Result<()> {
        Ok(())
    }
}
