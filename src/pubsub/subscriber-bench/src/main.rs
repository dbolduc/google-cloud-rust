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

use google_cloud_pubsub::client::Subscriber;
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

const NUM_STREAMS: usize = 1;
const NUM_CHANNELS: usize = 1;
const TEST_DURATION: Duration = Duration::from_secs(30);
//const NUM_STREAMS: usize = 16;
//const NUM_CHANNELS: usize = 16;
//const TEST_DURATION: Duration = Duration::from_secs(30);

//#[tokio::main(flavor = "multi_thread", worker_threads = 16)]
#[tokio::main(flavor = "multi_thread", worker_threads = 1)]
async fn main() -> anyhow::Result<()> {
    let _guard = trace_guard();

    println!("std::thread::available_parallelism()={}", std::thread::available_parallelism()?.get());

    let client = Subscriber::builder()
            .with_grpc_subchannel_count(NUM_CHANNELS)
            .build().await?;

    // Wait for N seconds then close the session.
    use tokio_util::sync::CancellationToken;
    let cancel = CancellationToken::new();
    let cancel_cloned = cancel.clone();
    tokio::spawn(async move {
        sleep(TEST_DURATION).await;
        tracing::info!("Closing subscriber session...");
        cancel_cloned.cancel();
    });

    // TODO : TIL : JoinSet. Would be nicer.
    let mut tasks: Vec<tokio::task::JoinHandle<google_cloud_pubsub::Result<i32>>> = Vec::new();
    for i in 0..NUM_STREAMS {
        let cancel = cancel.clone();
        //let client = clients.get(i % NUM_CLIENTS).expect("always valid");
        let client = client.clone();

        tracing::info!("Opening subscriber session {i}.");
        let mut session = client
            //.streaming_pull("projects/dbolduc-test/subscriptions/subscription-id")
            .streaming_pull("projects/dbolduc-test/subscriptions/help-me-andrew-browne")
            .set_max_outstanding_messages(200_000)
            .set_max_outstanding_bytes(200_000_000)
            .start()
            .await?;
        tracing::info!("Opened subscriber session {i}.");
        tasks.push(tokio::spawn(async move {
            let mut ack_count = 0;
            while let Some(r) = tokio::select! {
                v = session.next() => v,
                _ = cancel.cancelled() => None,
            } {
                let (m, h) = r?;
                tracing::info!("Application received message={m:?}.");
                h.ack();
                ack_count += 1;
            }

            //let status = session.close().await;
            tracing::info!("Subscriber session {i} is closed.");
            Ok(ack_count)
        }))
    }
    let mut ack_count = 0;
    for t in tasks {
        ack_count += t.await??;
    }

    println!("Acked {ack_count} messages in {TEST_DURATION:?}, via {NUM_STREAMS} streams/clients.");

    Ok(())
}
