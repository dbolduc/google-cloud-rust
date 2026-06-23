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

use crate::Result;
use crate::google::cloud::bigquery::storage::v1::{AppendRowsRequest, AppendRowsResponse};
use tokio::sync::{mpsc, oneshot};

pub(super) struct AppendRequest {
    pub request: AppendRowsRequest,
    pub resp_tx: oneshot::Sender<Result<AppendRowsResponse>>,
}

pub(super) enum StreamCommand {
    Append(AppendRequest),
}

/// A handle to a task running a stream.
#[derive(Clone)]
pub(super) struct StreamHandle {
    pub tx: mpsc::Sender<StreamCommand>,
}

pub(super) struct ConnectionPool {
    // For now, we only handle the default stream.
    // In the future, this will be a more complex manager for multiplexing and auto-scaling.
    default_stream: tokio::sync::OnceCell<StreamHandle>,
}

impl ConnectionPool {
    pub fn new() -> Self {
        Self {
            default_stream: tokio::sync::OnceCell::new(),
        }
    }

    pub async fn get_or_init_default<F, Fut>(&self, init: F) -> Result<StreamHandle>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<StreamHandle>>,
    {
        if let Some(handle) = self.default_stream.get() {
            return Ok(handle.clone());
        }

        let handle = init().await?;
        // Ignore the error if another thread initialized it in the meantime.
        let _ = self.default_stream.set(handle.clone());
        Ok(handle)
    }
}
