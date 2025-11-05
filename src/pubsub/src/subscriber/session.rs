// Copyright 2025 Google LLC
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

use super::builder::Subscribe;
use super::model::Message;
use super::transport::TransportStub;
use crate::Result;
use crate::google::pubsub::v1::{self, StreamingPullResponse};
use futures::stream::unfold;
use gax::options::RequestOptions;
use std::sync::Arc;
use tokio::sync::Notify;

fn keepalive() -> v1::StreamingPullRequest {
    v1::StreamingPullRequest::default()
}

pub(crate) type TonicStream = tonic::Response<tonic::codec::Streaming<v1::StreamingPullResponse>>;
pub(crate) type TonicStreamInner = tonic::codec::Streaming<v1::StreamingPullResponse>;

// TODO : just open a stream that returns some response.
async fn open_stream(
    inner: Arc<TransportStub>,
    initial_req: v1::StreamingPullRequest,
    close_signal: Arc<Notify>,
) -> Result<TonicStream> {
    enum State {
        Initial {
            req: v1::StreamingPullRequest,
            close_signal: Arc<Notify>,
        },
        Open {
            close_signal: Arc<Notify>,
        },
    }

    // A write source that sends one request, waits N seconds, then closes the stream.
    let request_stream = unfold(
        State::Initial {
            req: initial_req,
            close_signal,
        },
        move |state| async move {
            match state {
                State::Initial { req, close_signal } => {
                    tracing::info!("Sending initial request.");
                    Some((req, State::Open { close_signal }))
                }
                State::Open { close_signal } => {
                    tracing::info!("Started sleeping.");
                    tokio::select! {
                        _ = tokio::time::sleep(tokio::time::Duration::from_secs(30)) => {
                            // This is a keepalive ping.
                            tracing::info!("Done sleeping. Sending keepalive ping.");
                            Some((keepalive(), State::Open { close_signal }))
                        },
                        _ = close_signal.notified() => {
                            // NOTE : There is an unlikely race here.
                            // We should create one `Notified` or `NotifiedOwned` total for the stream, instead of one every 30 seconds.
                            // The sleep branch of the select can hit, then the session is closed, and we will miss the notification. The stream stays open.
                            // I spent an hour trying to fix this, but failed. I will defer the fix. It is not important for the design.
                            tracing::info!("Done sleeping. Closing stream.");
                            None
                        },
                    }
                }
            }
        },
    );

    tracing::info!("Opening subscriber session.");
    let stream = inner
        .streaming_pull(request_stream, RequestOptions::default())
        .await?;
    tracing::info!("Subscriber session is open.");

    Ok(stream)
}

/// Represents an open subscribe session.
pub struct SubscribeSession {
    //inner: Arc<TransportStub>,
    close_signal: Arc<tokio::sync::Notify>,
    stream: TonicStreamInner,
    pool: MessagePool,
}

impl SubscribeSession {
    pub(crate) async fn new(builder: Subscribe) -> Result<Self> {
        let inner = builder.inner;
        let initial_req = {
            let mut r = v1::StreamingPullRequest::default();
            r.subscription = builder.subscription;
            // TODO : unhardcoded other settings
            r.stream_ack_deadline_seconds = 10;
            r.max_outstanding_messages = 1000_i64;
            r.max_outstanding_bytes = 104857600_i64;
            // TODO : generate client ID
            r.client_id = "darren".to_string();
            r
        };
        let close_signal = Arc::new(Notify::new());
        // TODO : we might need to open the stream in a task. I think I end up losing a single thread.
        let stream = open_stream(inner, initial_req, close_signal.clone())
            .await?
            // TODO : retries on errors
            .into_inner();
        let pool = MessagePool::default();
        let session = Self {
            //inner,
            close_signal,
            stream,
            pool,
        };
        Ok(session)
    }

    pub async fn next(&mut self) -> Option<Message> {
        // TODO : synchronization. This feels reckless.
        while self.pool.messages.is_empty() {
            self.next_stream().await.ok()?;
        }
        self.pool.messages.pop_front()
    }

    async fn next_stream(&mut self) -> Result<()> {
        tracing::info!("Fetching more messages from stream.");
        use futures::StreamExt;
        if let Some(resp) = self.stream.next().await {
            // move messages to the pool
            self.pool
                .digest(resp.map_err(crate::Error::io /* TODO : wrong error mapping */)?);
        } else {
            // Stream has shutdown. Maybe restart the stream.
        }

        Ok(())
    }

    /// Closes the stream.
    ///
    /// Returns the final status of the stream, which always errors.
    ///
    /// TODO : pick a default behavior for shutdown.
    ///
    // TODO : perhaps we want shutdown options, but we are nowhere near ready for that.
    // TODO : perhaps we want to return an rpc::Status, because it is never Ok. I am mainly worried that the Result is misleading when it can never be Ok.
    pub async fn close(self) -> crate::Result<()> {
        tracing::info!("Closing stream.");
        // As of now there is one waiter: in the stream.
        self.close_signal.notify_waiters();

        Ok(())
    }
}

use std::collections::VecDeque;
// A message pool.
//
// This thing will likely do the lease management, when I am ready.
#[derive(Default)]
struct MessagePool {
    messages: VecDeque<Message>,
}

impl MessagePool {
    fn digest(&mut self, resp: StreamingPullResponse) {
        for m in resp.received_messages {
            tracing::info!("Added message={m:?} to the pool.");
            let message = m.message.unwrap();
            self.messages.push_back(Message {
                data: message.data,
                message_id: message.message_id,
                ack_id: m.ack_id,
            });
        }
    }
}
