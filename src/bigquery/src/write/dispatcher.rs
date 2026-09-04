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

// TODO : DARREN : THIS CODE HAS NOT BEEN VETTED.
// TODO : DARREN : THIS CODE HAS NOT BEEN VETTED.
// TODO : DARREN : THIS CODE HAS NOT BEEN VETTED.
// TODO : DARREN : THIS CODE HAS NOT BEEN VETTED.
// TODO : DARREN : THIS CODE HAS NOT BEEN VETTED.
// TODO : DARREN : THIS CODE HAS NOT BEEN VETTED.
// TODO : DARREN : THIS CODE HAS NOT BEEN VETTED.
// TODO : DARREN : THIS CODE HAS NOT BEEN VETTED.

use super::entry::StreamEntry;
use super::error::{AppendError, AppendResult};
use super::pool::StreamPool;
use super::runner::WriteRequest;
use arc_swap::ArcSwap;
use google_cloud_gax::error::rpc::Code;
use prost::Message;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::oneshot;

/// Dispatches append requests with connection affinity and atomic failover.
#[derive(Debug)]
pub(crate) struct Dispatcher {
    pub(crate) pool: Arc<StreamPool>,
    pub(crate) cached_stream: ArcSwap<StreamEntry>,
}

impl Dispatcher {
    /// Creates a new [Dispatcher] initialized with a stream from the pool.
    pub(crate) fn new(pool: Arc<StreamPool>, initial_stream: StreamEntry) -> Self {
        Self {
            pool,
            cached_stream: ArcSwap::from_pointee(initial_stream),
        }
    }

    /// Sends a request over the sticky connection. Evicts and updates stream cache on transient errors.
    pub(crate) async fn send(
        &self,
        req: crate::google::cloud::bigquery::storage::v1::AppendRowsRequest,
    ) -> AppendResult<crate::google::cloud::bigquery::storage::v1::AppendRowsResponse> {
        let stream = self.cached_stream.load_full();
        let stream_id = stream.id;

        match stream.send(req).await {
            Ok(resp) => Ok(resp),
            Err(err) => {
                if is_transient_error(&err) {
                    // Atomically evicts failed_id and returns the new replacement or least loaded candidate
                    let new_stream = self.pool.evict_and_replace(stream_id);

                    // Ensure we do not overwrite a newer stream swapped in by a concurrent task
                    let _ = self
                        .cached_stream
                        .compare_and_swap(&stream, Arc::new(new_stream));

                    // TODO: Retries
                }
                Err(err)
            }
        }
    }
}

// TODO : DARREN : THIS CODE HAS NOT BEEN VETTED.
pub(crate) fn is_transient_error(err: &AppendError) -> bool {
    match err {
        AppendError::UnexpectedEndOfStream => true,
        AppendError::RowErrors(_) => false,
        AppendError::Rpc { source } => {
            if let Some(status) = source.status() {
                matches!(
                    status.code,
                    Code::Aborted
                        | Code::DeadlineExceeded
                        | Code::Internal
                        | Code::ResourceExhausted
                        | Code::Unavailable
                        | Code::Unknown
                )
            } else {
                true
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::runner::tests::*;
    use super::super::transport::tests::*;
    use super::*;
    use bigquery_grpc_mock::{MockBigQueryWrite, start};
    use gaxi::grpc::tonic::Response as TonicResponse;
    use test_case::test_case;
    use tokio::sync::oneshot;
    use tokio::task::JoinSet;

    #[tokio::test]
    async fn todo() -> anyhow::Result<()> {
        Ok(())
    }
}
