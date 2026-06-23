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

use crate::pool::StreamCommand;
use crate::transport::Transport;
use crate::{Error, Result};
use gaxi::grpc::from_status::to_gax_error;
use google_cloud_gax::error::rpc::{Code, Status};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

pub(super) struct StreamTask {
    transport: Arc<Transport>,
    pending_responses: VecDeque<
        tokio::sync::oneshot::Sender<
            Result<crate::google::cloud::bigquery::storage::v1::AppendRowsResponse>,
        >,
    >,
}

impl StreamTask {
    pub(super) fn new(transport: Arc<Transport>) -> Self {
        Self {
            transport,
            pending_responses: VecDeque::new(),
        }
    }

    pub(super) async fn run(
        &mut self,
        mut command_rx: mpsc::Receiver<StreamCommand>,
    ) -> Result<()> {
        // Wait for the first append request to open the stream.
        let first_request = match command_rx.recv().await {
            Some(StreamCommand::Append(req)) => req,
            None => return Ok(()),
        };

        let (grpc_request_tx, grpc_request_rx) = mpsc::channel(100);

        // The first request must have the write_stream set.
        let request_params = format!("write_stream={}", first_request.request.write_stream);

        // Send the first request to the gRPC stream channel before opening the stream
        // to avoid deadlock with servers that wait for the first request.
        grpc_request_tx
            .send(first_request.request)
            .await
            .map_err(|_| {
                Error::service(
                    Status::default()
                        .set_code(Code::Internal)
                        .set_message("internal channel closed"),
                )
            })?;
        self.pending_responses.push_back(first_request.resp_tx);

        tracing::info!("Opening AppendRows stream");
        let mut grpc_response_stream = self
            .transport
            .append_rows(
                &request_params,
                grpc_request_rx,
                crate::RequestOptions::default(),
            )
            .await?
            .into_inner();

        loop {
            tokio::select! {
                Some(command) = command_rx.recv() => {
                    match command {
                        StreamCommand::Append(req) => {
                            tracing::debug!("Received request for append");
                            self.pending_responses.push_back(req.resp_tx);
                            if grpc_request_tx.send(req.request).await.is_err() {
                                tracing::error!("Failed to send request to gRPC stream");
                                break;
                            }
                        }
                    }
                }
                Some(response) = grpc_response_stream.next() => {
                    tracing::debug!("Received response from gRPC stream");
                    if let Some(pending) = self.pending_responses.pop_front() {
                        let _ = pending.send(response.map_err(to_gax_error));
                    }
                }
                else => {
                    tracing::info!("Stream closed");
                    break;
                }
            }
        }

        // Notify remaining pending requests that the stream has closed
        while let Some(pending) = self.pending_responses.pop_front() {
            let _ = pending.send(Err(Error::service(
                Status::default()
                    .set_code(Code::Cancelled)
                    .set_message("stream closed"),
            )));
        }

        Ok(())
    }
}
