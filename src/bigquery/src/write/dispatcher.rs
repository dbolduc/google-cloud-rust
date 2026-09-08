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

use super::append_response::{AppendResponse, to_result};
use super::entry::StreamEntry;
use super::error::{AppendError, AppendResult};
use super::pool::StreamPool;
use crate::Error;
use crate::model::AppendRowsRequest;
use arc_swap::ArcSwap;
use gaxi::prost::{FromProto, ToProto};
use google_cloud_gax::error::rpc::Code;
use std::sync::Arc;

/// Dispatches append requests with connection affinity and atomic failover.
#[derive(Debug)]
pub(crate) struct Dispatcher {
    pub(crate) pool: Arc<StreamPool>,
    pub(crate) cached_stream: ArcSwap<StreamEntry>,
}

impl Dispatcher {
    /// Creates a new [Dispatcher] initialized with a stream from the pool.
    pub(crate) fn new(pool: Arc<StreamPool>) -> Self {
        let stream = pool.get();
        Self {
            pool,
            cached_stream: ArcSwap::from_pointee(stream),
        }
    }

    /// Sends a request over the sticky connection. Evicts and updates stream cache on transient errors.
    pub(crate) async fn send(&self, req: AppendRowsRequest) -> AppendResult<AppendResponse> {
        let req = req.to_proto().map_err(Error::deser)?;

        let stream = self.cached_stream.load_full();
        let stream_id = stream.id;

        let resp = match stream.send(req).await {
            Ok(resp) => Ok(resp),
            Err(err) => {
                if is_transient_error(&err) {
                    // Atomically evicts failed_id and returns the new replacement or least loaded candidate
                    let new_stream = self.pool.evict_and_replace(stream_id);

                    // Ensure we do not overwrite a newer stream swapped in by a concurrent task
                    let _ = self
                        .cached_stream
                        .compare_and_swap(&stream, Arc::new(new_stream));

                    // TODO(#6355): implement retries
                }
                Err(err)
            }
        }?;

        let resp = resp.cnv().map_err(Error::ser)?;
        to_result(resp)
    }
}

pub(crate) fn is_transient_error(err: &AppendError) -> bool {
    match err {
        AppendError::UnexpectedEndOfStream => true,
        AppendError::RowErrors(_) => false,
        // TODO(#6355): classify transient RPC errors
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
    #[tokio::test]
    async fn todo() -> anyhow::Result<()> {
        Ok(())
    }
}
