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
// See the License for the_project/LICENSE.

use crate::error::{AppendError, AppendResult};
use crate::pool::{StreamEntry, StreamPool};
use crate::runner::WriteRequest;
use arc_swap::ArcSwap;
use google_cloud_gax::error::rpc::Code;
use prost::Message;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::sync::oneshot;

pub(crate) struct LoadGuard {
    outstanding_requests: Arc<std::sync::atomic::AtomicU64>,
    outstanding_bytes: Arc<std::sync::atomic::AtomicU64>,
    bytes: u64,
}

#[allow(dead_code)]
impl LoadGuard {
    pub(crate) fn new(
        outstanding_requests: Arc<std::sync::atomic::AtomicU64>,
        outstanding_bytes: Arc<std::sync::atomic::AtomicU64>,
        bytes: u64,
    ) -> Self {
        outstanding_requests.fetch_add(1, Ordering::Relaxed);
        outstanding_bytes.fetch_add(bytes, Ordering::Relaxed);
        Self {
            outstanding_requests,
            outstanding_bytes,
            bytes,
        }
    }
}

impl Drop for LoadGuard {
    fn drop(&mut self) {
        self.outstanding_requests
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        self.outstanding_bytes
            .fetch_sub(self.bytes, std::sync::atomic::Ordering::Relaxed);
    }
}

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

/// Layer 1: Raw Dispatcher (Pure gRPC channel send & await)
#[derive(Debug, Default)]
pub(crate) struct RawStreamDispatcher;

impl RawStreamDispatcher {
    pub(crate) async fn dispatch(
        &self,
        stream: &StreamEntry,
        req: crate::google::cloud::bigquery::storage::v1::AppendRowsRequest,
    ) -> AppendResult<crate::google::cloud::bigquery::storage::v1::AppendRowsResponse> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let write = WriteRequest { req, resp_tx };

        stream
            .sender
            .send(write)
            .await
            .map_err(|_| AppendError::UnexpectedEndOfStream)?;

        match resp_rx.await {
            Ok(Ok(resp)) => Ok(resp),
            Ok(Err(err)) => Err(err),
            Err(_) => Err(AppendError::UnexpectedEndOfStream),
        }
    }
}

/// Layer 2: Load Guarded Dispatcher (Atomically tracks traffic load)
#[derive(Debug, Default)]
pub(crate) struct LoadGuardedDispatcher {
    inner: RawStreamDispatcher,
}

impl LoadGuardedDispatcher {
    pub(crate) async fn dispatch(
        &self,
        stream: &StreamEntry,
        req: crate::google::cloud::bigquery::storage::v1::AppendRowsRequest,
    ) -> AppendResult<crate::google::cloud::bigquery::storage::v1::AppendRowsResponse> {
        let req_len = req.encoded_len() as u64;

        let _guard = LoadGuard::new(
            stream.outstanding_requests.clone(),
            stream.outstanding_bytes.clone(),
            req_len,
        );

        self.inner.dispatch(stream, req).await
    }
}

/// Layer 3: Connection Dispatcher (Coordinates failover routing and connection eviction)
#[derive(Debug)]
pub(crate) struct Dispatcher {
    pub(crate) pool: Arc<StreamPool>,
    pub(crate) cached_stream: ArcSwap<StreamEntry>,
    dispatcher: LoadGuardedDispatcher,
}

impl Dispatcher {
    /// Creates a new [Dispatcher].
    pub(crate) fn new(pool: Arc<StreamPool>, cached_stream: ArcSwap<StreamEntry>) -> Self {
        Self {
            pool,
            cached_stream,
            dispatcher: LoadGuardedDispatcher::default(),
        }
    }

    /// Sends a request, routing it onto the active stream and handling failover eviction on transient errors.
    pub(crate) async fn send_request(
        &self,
        req: crate::google::cloud::bigquery::storage::v1::AppendRowsRequest,
    ) -> AppendResult<crate::google::cloud::bigquery::storage::v1::AppendRowsResponse> {
        let stream = self.cached_stream.load_full();
        let stream_id = stream.id;

        match self.dispatcher.dispatch(&stream, req).await {
            Ok(resp) => Ok(resp),
            Err(err) => {
                if is_transient_error(&err) {
                    self.pool.evict_and_replace(stream_id);
                    let new_stream = self.pool.get_least_loaded_stream();
                    // 3. Atomically update OUR cache ONLY if another task hasn't 
                    //    already updated it to a newer stream while we were waiting.
                    let _ = self.cached_stream.compare_and_swap(&stream, Arc::new(new_stream));
                }
                // TODO : Retries
                Err(err)
            }
        }
    }
}
