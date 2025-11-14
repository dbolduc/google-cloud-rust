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
use super::leaser::DefaultLeaser;
use super::leaser::LeaseManager;
use super::model::Message;
use super::transport::TransportStub;
use crate::Result;
use crate::google::pubsub::v1::{self, StreamingPullResponse};
use futures::StreamExt as _;
use futures::stream::unfold;
use gax::options::RequestOptions;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

fn keepalive() -> v1::StreamingPullRequest {
    v1::StreamingPullRequest::default()
}

pub(crate) type TonicStream = tonic::Response<tonic::codec::Streaming<v1::StreamingPullResponse>>;
pub(crate) type TonicStreamInner = tonic::codec::Streaming<v1::StreamingPullResponse>;

async fn open_stream(
    inner: Arc<TransportStub>,
    initial_req: v1::StreamingPullRequest,
    shutdown: CancellationToken,
) -> Result<TonicStream> {
    enum State {
        Initial {
            req: v1::StreamingPullRequest,
            shutdown: CancellationToken,
        },
        Open {
            shutdown: CancellationToken,
        },
    }

    // A write source that sends one request, waits N seconds, then closes the stream.
    let request_stream = unfold(
        State::Initial {
            req: initial_req,
            shutdown,
        },
        move |state| async move {
            match state {
                State::Initial { req, shutdown } => {
                    tracing::info!("Sending initial request.");
                    Some((req, State::Open { shutdown }))
                }
                State::Open { shutdown } => {
                    tracing::info!("Started sleeping.");
                    tokio::select! {
                        _ = tokio::time::sleep(tokio::time::Duration::from_secs(30)) => {
                            // This is a keepalive ping.
                            tracing::info!("Done sleeping. Sending keepalive ping.");
                            Some((keepalive(), State::Open { shutdown }))
                        },
                        _ = shutdown.cancelled() => {
                            tracing::info!("Shutdown: Stream.");
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
    stream: TonicStreamInner,
    pool: MessagePool,
    leaser: LeaseManager,
    shutdown: CancellationToken,
}

impl SubscribeSession {
    pub(crate) async fn new(builder: Subscribe) -> Result<Self> {
        let inner = builder.inner;
        let initial_req = {
            let mut r = v1::StreamingPullRequest::default();
            r.subscription = builder.subscription;
            // TODO : unhardcoded other settings
            r.stream_ack_deadline_seconds = 10;
            // TODO : testing more generous defaults
            r.max_outstanding_messages = 200000_i64;
            r.max_outstanding_bytes = 200000000_i64;
            r.client_id = uuid::Uuid::new_v4().to_string();
            r
        };
        let shutdown = CancellationToken::new();
        // TODO : we might need to open the stream in a task. I think I end up losing a single thread.
        let stream = open_stream(inner.clone(), initial_req, shutdown.clone())
            .await?
            // TODO : retries on errors
            .into_inner();
        let pool = MessagePool::default();
        let leaser = LeaseManager::new(shutdown.clone(), DefaultLeaser::new(inner));
        let session = Self {
            //inner,
            stream,
            pool,
            leaser,
            shutdown,
        };
        Ok(session)
    }

    pub async fn next(&mut self) -> Option<Message> {
        // TODO : It feels like we should be constantly pulling messages from
        // the stream in a backgroud task (up to some limit), instead of waiting
        // for the application to ask for more?

        // NOTE : from team discussion:
        // - we should only consider a max of one response. (The responses are configured to contain N messages. That is already a buffer)
        // - gRPC might buffer reads on the socket. So we are not saving anything with an additional buffer.

        // TODO : synchronization? This feels reckless.
        // Should it even be possible?? I guess it is allowed because `next_stream` is inline.
        // Maybe the compiler will stop me later when I try to do more complicated things?.
        while self.pool.messages.is_empty() {
            self.next_stream().await.ok()?;
        }
        self.pool.messages.pop_front()
    }

    async fn next_stream(&mut self) -> Result<()> {
        tracing::info!("Fetching more messages from stream.");
        if let Some(resp) = self.stream.next().await {
            // move messages to the pool
            let resp = resp.map_err(crate::Error::io /* TODO : wrong error mapping */)?;
            self.on_read(resp).await;
        } else {
            // Stream has shutdown. Maybe restart the stream.
        }

        Ok(())
    }

    async fn on_read(&mut self, resp: StreamingPullResponse) {
        for m in resp.received_messages {
            tracing::info!("Added message={m:?} to the pool.");
            self.leaser.new_message_tx().send(m.ack_id.clone());

            let message = m.message.unwrap();
            self.pool.messages.push_back(Message {
                data: message.data,
                message_id: message.message_id,
                ack_id: m.ack_id,
                ack_tx: self.leaser.ack_tx(),
            });
        }
    }

    /// Closes the stream.
    ///
    /// Returns the final status of the stream, which always errors. (the happy
    /// case receives a CANCELLED).
    ///
    /// TODO : pick a default behavior for shutdown.
    /// TODO : perhaps we want shutdown options, but we are nowhere near ready for that.
    pub async fn close(self) -> crate::Result<()> {
        tracing::info!("Shutdown: application triggered shutdown via close().");
        self.shutdown.cancel();
        self.leaser.close().await?;
        // Drain stream? TODO : good idea or no?
        let mut stream = self.stream;
        while stream.next().await.is_some() {}

        Ok(())
    }
}

use std::collections::VecDeque;
// A message pool.
//
// This thing is separate from the lease management (which only knows/cares about ack IDs)
//
// TODO : This thing gets more complicated when we introduce ordering keys.
// I suspect we get a `Hashmap<String, VecDeque<Message>>`.
//
// Q: which order do we serve messages from the many queues? Like obviously we
// go in order for a given queue. Are there fairness considerations?
#[derive(Default)]
struct MessagePool {
    messages: VecDeque<Message>,
}
