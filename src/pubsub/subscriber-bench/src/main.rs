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

use futures::StreamExt as _;
use google_cloud_pubsub::subscriber::client::Subscriber;
use tokio::time::{Duration, sleep};

fn trace_guard() -> tracing::dispatcher::DefaultGuard {
    use tracing_subscriber::fmt::format::FmtSpan;
    let subscriber = tracing_subscriber::fmt()
        .with_level(true)
        .with_thread_ids(true)
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .finish();

    tracing::subscriber::set_default(subscriber)
}

const NUM_TASKS: usize = 256;
const NUM_CLIENTS: usize = 16;
const TEST_DURATION: Duration = Duration::from_secs(600);
//const NUM_TASKS: usize = 1;
//const NUM_CLIENTS: usize = 1;
//const TEST_DURATION: Duration = Duration::from_secs(5);

async fn make_clients() -> Vec<Subscriber> {
    let client_fs: Vec<_> = (0..NUM_CLIENTS).map(|_| Subscriber::new()).collect();
    futures::future::join_all(client_fs)
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect()
}

#[cfg(not(feature = "raw-grpc"))]
#[tokio::main(flavor = "multi_thread", worker_threads = 16)]
//#[tokio::main(flavor="current_thread")]
async fn main() -> anyhow::Result<()> {
    //let _guard = trace_guard();

    tracing::info!("Creating clients...");
    let clients = make_clients().await;
    tracing::info!("Creating clients... DONE.");

    // Wait for N seconds then close the session.
    use tokio_util::sync::CancellationToken;
    let cancel = CancellationToken::new();
    let cancel_cloned = cancel.clone();
    tokio::spawn(async move {
        sleep(TEST_DURATION).await;
        tracing::info!("Closing subscriber session...");
        cancel_cloned.cancel();
    });

    let mut tasks = Vec::new();
    for i in 0..NUM_TASKS {
        let cancel = cancel.clone();
        let client = clients.get(i % NUM_CLIENTS).expect("always valid");

        tracing::info!("Opening subscriber session {i}.");
        let mut session = client
            .subscribe("projects/dbolduc-test/subscriptions/subscription-id")
            .start()
            .await?;
        tasks.push(tokio::spawn(async move {
            let mut ack_count = 0;
            while let Some(m) = tokio::select! {
                v = session.next() => v,
                _ = cancel.cancelled() => None,
            } {
                tracing::info!("Application received message={m:?}.");
                m.ack();
                ack_count += 1;
            }

            let status = session.close().await;
            tracing::info!("Subscriber session {i} is closed. Status={status:?}");
            ack_count
        }))
    }
    let mut ack_count = 0;
    for t in tasks {
        ack_count += t.await?;
    }

    println!("Acked {ack_count} messages in {TEST_DURATION:?}, via {NUM_CLIENTS} streams/clients.");

    Ok(())
}

use tonic::Extensions;
use tonic::metadata::{Ascii, MetadataMap, MetadataValue};
use tonic::transport::{Channel, Endpoint, Uri, channel::ClientTlsConfig};
type GrpcClient = grpc::google::pubsub::v1::subscriber_client::SubscriberClient<Channel>;

async fn make_grpc_clients() -> Vec<GrpcClient> {
    let ep = Endpoint::from_static("https://pubsub.googleapis.com")
        .origin(Uri::from_static("https://pubsub.googleapis.com"))
        .tls_config(ClientTlsConfig::new().with_enabled_roots())
        .unwrap();

    return (0..NUM_CLIENTS)
        .map(|_| GrpcClient::new(ep.connect_lazy()))
        .collect();
    let channel_fs: Vec<_> = (0..NUM_CLIENTS).map(|_| ep.connect()).collect();
    futures::future::join_all(channel_fs)
        .await
        .into_iter()
        .map(|r| GrpcClient::new(r.unwrap()))
        .collect()
}

#[cfg(feature = "raw-grpc")]
#[tokio::main(flavor = "multi_thread", worker_threads = 16)]
//#[tokio::main(flavor="current_thread")]
async fn main() -> anyhow::Result<()> {
    //let _guard = trace_guard();

    let token = std::env::var("TOKEN").unwrap();

    let clients = make_grpc_clients().await;

    // Wait for N seconds then close the session.
    use tokio::time::{Duration, sleep};
    use tokio_util::sync::CancellationToken;
    let cancel = CancellationToken::new();
    let cancel_cloned = cancel.clone();
    tokio::spawn(async move {
        sleep(TEST_DURATION).await;
        tracing::info!("Closing subscriber session...");
        cancel_cloned.cancel();
    });

    let mut tasks = Vec::new();
    for i in 0..NUM_TASKS {
        let cancel = cancel.clone();
        let mut client = clients.get(i % NUM_CLIENTS).expect("always valid").clone();

        tracing::info!("Opening subscriber stream {i}.");
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        let initial_req = {
            use grpc::google::pubsub::v1::StreamingPullRequest;
            let mut r = StreamingPullRequest::default();
            r.subscription = "projects/dbolduc-test/subscriptions/subscription-id".to_string();
            r.stream_ack_deadline_seconds = 10;
            r.max_outstanding_messages = 1000_i64;
            r.max_outstanding_bytes = 104857600_i64;
            r.client_id = uuid::Uuid::new_v4().to_string();
            r
        };
        tx.send(initial_req).await?;
        let outbound = tokio_stream::wrappers::ReceiverStream::new(rx);

        let mut headers = MetadataMap::new();
        let bearer = format!("Bearer {token}");
        headers.insert("authorization", bearer.parse()?);
        let extensions = {
            let mut e = tonic::Extensions::new();
            e.insert(tonic::GrpcMethod::new(
                "google.pubsub.v1.Subscriber",
                "StreamingPull",
            ));
            e
        };
        let mut stream = client
            .streaming_pull(tonic::Request::from_parts(headers, extensions, outbound))
            .await?
            .into_inner();
        tasks.push(tokio::spawn(async move {
            let mut ack_count = 0;
            while let Some(m) = tokio::select! {
                v = stream.next() => v,
                _ = cancel.cancelled() => None,
            } {
                tracing::info!("Application received message={m:?}.");
                // No sending acks at the moment.
                ack_count += 1;
            }

            drop(tx); // Closes the stream.
            while let Some(m) = stream.next().await {}
            tracing::info!("Subscriber stream {i} is closed. Status=???");
            ack_count
        }))
    }
    let mut ack_count = 0;
    for t in tasks {
        ack_count += t.await?;
    }

    println!("Acked {ack_count} messages in {TEST_DURATION:?}, via {NUM_CLIENTS} streams/clients.");

    Ok(())
}
