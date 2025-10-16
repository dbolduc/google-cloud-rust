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
use crate::google::pubsub::v1;
use futures::Stream;
use futures::stream::unfold;
use gax::options::RequestOptions;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Notify;
use tokio::sync::futures::Notified;
use tokio::sync::futures::OwnedNotified;
use tokio::task::JoinHandle;

fn keepalive() -> v1::StreamingPullRequest {
    v1::StreamingPullRequest::default()
}

pub(crate) type TonicStream = tonic::Response<tonic::codec::Streaming<v1::StreamingPullResponse>>;

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
    //stream: TonicStream,
    read_loop: JoinHandle<Result<()>>,
}

impl SubscribeSession {
    pub(crate) async fn new<C, F>(builder: Subscribe<C, F>) -> Result<Self>
    where
        C: Fn(Message) -> F + Send + 'static,
        F: Future<Output = ()> + Send + 'static,
    {
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
        let mut stream = open_stream(inner, initial_req, close_signal.clone())
            .await?
            .into_inner();
        let callback = builder.callback;
        let read_loop: JoinHandle<Result<()>> = tokio::spawn(async move {
            while let Some(resp) = stream
                .message()
                .await
                // TODO : probably the wrong error mapping.
                .map_err(crate::Error::io)?
            {
                tracing::info!("Received message {resp:?}");
                for msg in resp.received_messages {
                    let m = msg.message.unwrap();
                    let id = m.message_id;
                    tracing::info!("Received message {id} with Ack ID: {}", msg.ack_id);

                    let message = Message {
                        data: m.data,
                        message_id: id,
                        ack_id: msg.ack_id,
                    };
                    // TODO : take control of the message
                    // e.g. send modack, execute message callback. etc.

                    // Execute callback
                    // TODO : I just fire and forget this task. We should be limiting the number of outstanidng tasks.
                    _ = tokio::task::spawn(callback(message));
                }
            }
            Ok(())
        });
        let session = Self {
            //inner,
            close_signal,
            //stream,
            read_loop,
        };
        Ok(session)
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
        self.read_loop.await.map_err(crate::Error::io)?
    }
}
