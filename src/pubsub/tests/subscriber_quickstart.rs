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

#[cfg(test)]
mod tests {
    use futures::stream::unfold;
    use gax::options::RequestOptions;
    use google_cloud_pubsub::Subscriber;
    use google_cloud_pubsub::google::pubsub::v1;
    use tokio::sync::mpsc::{Receiver, channel};

    fn initial_request() -> v1::StreamingPullRequest {
        let mut r = v1::StreamingPullRequest::default();
        r.subscription = "projects/dbolduc-test/subscriptions/subscription-id".to_string();
        r.stream_ack_deadline_seconds = 10;
        r.client_id = "darren".to_string();
        r.max_outstanding_messages = 1000_i64;
        r.max_outstanding_bytes = 104857600_i64;
        r
    }

    fn ack_request(ack_id: String) -> v1::StreamingPullRequest {
        let mut r = v1::StreamingPullRequest::default();
        r.ack_ids.push(ack_id);
        r
    }

    //#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[tokio::test]
    async fn quickstart() -> anyhow::Result<()> {
        // Enable a basic subscriber. Useful to troubleshoot problems and visually
        // verify tracing is doing something.
        let _guard = {
            use tracing_subscriber::fmt::format::FmtSpan;
            let subscriber = tracing_subscriber::fmt()
                .with_level(true)
                .with_thread_ids(true)
                .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
                .finish();

            tracing::subscriber::set_default(subscriber)
        };

        let client = Subscriber::new(gaxi::options::ClientConfig::default()).await?;

        // A write source that sends one request, waits 15 seconds, then closes the stream.
        let _open_only_stream = unfold(true, move |initial| async move {
            if initial {
                Some((initial_request(), false))
            } else {
                tracing::info!("Started sleeping.");
                tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                tracing::info!("Done sleeping.");
                None
            }
        });

        // A channel for passing aroud Ack IDs. Used for convenience.
        let (tx, rx) = channel(32);
        struct State {
            initial: bool,
            ack_count: u32,
            max_acks: u32,
            receiver: Receiver<String>,
        }

        impl State {
            fn new(receiver: Receiver<String>, max_acks: u32) -> Self {
                State {
                    initial: true,
                    ack_count: 0,
                    max_acks,
                    receiver,
                }
            }
        }

        // A write source that sends one request, waits for N incoming messages,
        // acks them (one at a time), then closes the stream.
        let state = State::new(rx, 1);
        let ack_n_stream = unfold(state, move |state| {
            async move {
                match state {
                    mut s if s.initial => {
                        // Send the initial request to open the stream
                        tracing::info!("Sending initial write.");
                        s.initial = false;
                        Some((initial_request(), s))
                    }
                    s if s.ack_count == s.max_acks => {
                        // We acked a message. No further writes, your honor.

                        // Closing the write stream immediately seems to race
                        // with sending the previous request. I don't know what
                        // I can `await` to make the sequencing correct.
                        //
                        // AFAICT, the server does not respond to every write.
                        // Maybe the gRPC channel knows something? If so, we
                        // would have to expose it via the gaxi interface. Ugh...
                        //
                        // For now, I will just sleep.
                        tracing::info!("Writes done - waiting to close.");
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        tracing::info!("Writes done - closing.");
                        None
                    }
                    mut s => {
                        // Await the next message to ack on the channel.
                        tracing::info!("Waiting for next message...");
                        let ack_id = s.receiver.recv().await?;
                        tracing::info!("Acking: {ack_id}");
                        s.ack_count += 1;
                        Some((ack_request(ack_id), s))
                    }
                }
            }
        });

        tracing::info!("Opening subscriber session.");
        let mut stream = client
            //.streaming_pull(open_only_stream, RequestOptions::default())
            .streaming_pull(ack_n_stream, RequestOptions::default())
            .await?
            .into_inner();
        tracing::info!("Subscriber session is open.");

        // Handle messages
        while let Some(resp) = stream.message().await? {
            tracing::info!("Received message {resp:?}");
            if let Some(acks) = resp.acknowledge_confirmation {
                for id in acks.ack_ids {
                    tracing::info!("Ack confirmation on message: {id}");
                }
            }
            for msg in resp.received_messages {
                let id = msg.message.unwrap().message_id;
                tracing::info!("Received message {id} with Ack ID: {}", msg.ack_id);

                // Send the ack id to the channel.
                //
                // In practice, we would probably send a unary Acknowledge RPC...
                // but that is not the point of this quickstart. The point of this
                // quickstart is to learn how to do client-side streaming.
                tx.send(msg.ack_id).await?;
            }
        }
        tracing::info!("Subscriber session closed.");

        Ok(())
    }
}
