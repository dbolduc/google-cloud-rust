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

use super::builder::StreamingPull;
use super::handler::{AckResult, AtLeastOnce, Handler};
use super::keepalive::spawn;
use super::lease_loop::LeaseLoop;
use super::lease_state::LeaseOptions;
use super::leaser::DefaultLeaser;
use super::stream::open_stream;
use super::stub::Stub;
use super::transport::Transport;
use crate::google::pubsub::v1::StreamingPullRequest;
use crate::model::PubsubMessage;
use crate::{Error, Result};
use std::collections::VecDeque;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Represents an open subscribe session.
///
/// This is a stream-like class for serving messages to an application.
pub struct Session {
    // TODO : Probably need to hold onto this when handling stream retries.
    //inner: Arc<Transport>,

    // TODO : should this be generic on the Stub for unit tests? Maybe.
    // But remember it is a public type. We do not want internal generics on it
    // if we can help it.
    stream: <Transport as Stub>::Stream,
    pool: VecDeque<(PubsubMessage, Handler)>,
    shutdown: CancellationToken,
    lease_loop: JoinHandle<()>,
    keepalive: JoinHandle<()>,
    message_tx: UnboundedSender<String>,
    ack_tx: UnboundedSender<AckResult>,
}

impl Session {
    pub(crate) async fn new(builder: StreamingPull<Transport>) -> Result<Self> {
        let shutdown = CancellationToken::new();
        let inner = builder.inner;
        let leaser = DefaultLeaser::new(
            inner.clone(),
            builder.subscription.clone(),
            builder.ack_deadline_seconds,
        );
        let LeaseLoop {
            handle: lease_loop,
            message_tx,
            ack_tx,
        } = LeaseLoop::new(leaser, LeaseOptions::default());

        // TODO : probably want this on the builder.
        let initial_req = StreamingPullRequest {
            subscription: builder.subscription.clone(),
            stream_ack_deadline_seconds: builder.ack_deadline_seconds,
            max_outstanding_messages: builder.max_outstanding_messages,
            max_outstanding_bytes: builder.max_outstanding_bytes,
            ..Default::default()
        };
        let (stream, request_tx) = open_stream(inner, initial_req).await?;

        let keepalive = spawn(request_tx, shutdown.clone());

        Ok(Self {
            stream,
            pool: VecDeque::new(),
            shutdown,
            lease_loop,
            keepalive,
            message_tx,
            ack_tx,
        })
    }

    // TODO : Document, this is public.
    pub async fn next(&mut self) -> Option<Result<(PubsubMessage, Handler)>> {
        loop {
            // Serve a message if we have one ready.
            if let Some(item) = self.pool.pop_front() {
                return Some(Ok(item));
            }
            // Otherwise, read more messages from the stream.
            if let Err(e) = self.stream_next().await? {
                return Some(Err(e));
            }
        }
    }

    pub(crate) async fn stream_next(&mut self) -> Option<Result<()>> {
        use super::stub::TonicStreaming as _;
        use gaxi::grpc::from_status::to_gax_error;
        use gaxi::prost::FromProto as _;
        let resp = match self.stream.next_message().await.transpose()? {
            Err(e) => return Some(Err(to_gax_error(e))),
            Ok(resp) => resp,
        };
        let result = move || {
            for rm in resp.received_messages {
                // TODO : when is the receiver dropped? Should be never.
                let _ = self.message_tx.send(rm.ack_id.clone());
                let pb = rm
                    .message
                    .expect("The message field is always present.")
                    .cnv()
                    .map_err(Error::deser)?;
                self.pool.push_back((
                    pb,
                    Handler::AtLeastOnce(AtLeastOnce {
                        ack_id: rm.ack_id,
                        ack_tx: self.ack_tx.clone(),
                    }),
                ));
            }
            Ok(())
        };
        Some(result())
    }

    // TODO : document
    // TODO : rewrite this and make it clean. dbolduc wrote the code when he was tired.
    /// Gracefully close the session.
    ///
    /// The subscribe session will flush all pending acknowledgements, and
    /// negatively acknowledge all other messages under lease management, so
    /// the messages can be resent to another client as soon as possible.
    ///
    /// Returns the final status of the stream.
    pub async fn close(self) -> Result<()> {
        // Close the stream
        self.shutdown.cancel();
        let _ = self.keepalive.await;

        // Drain the stream, without returning messages to the application
        use super::stub::TonicStreaming;
        let mut stream = self.stream;
        while let Some(result) = stream.next_message().await.transpose() {
            match result {
                Ok(resp) => {
                    // give messages to leaser.
                    for rm in resp.received_messages {
                        let _ = self.message_tx.send(rm.ack_id.clone());
                    }
                }
                Err(_e) => {
                    // TODO : save the error, return it from here.
                }
            }
        }

        // We are done with new messages. Drop the channel.
        //
        // This triggers a shutdown of the lease loop, which flushes all pending
        // acks and nacks all other messages under lease.
        drop(self.message_tx);
        let _ = self.lease_loop.await;

        Ok(())
    }
}
