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
use tokio::task::JoinSet;

fn enable_tracing() {
    use tracing_subscriber::fmt::format::FmtSpan;
    use tracing_subscriber::util::SubscriberInitExt;
    tracing_subscriber::fmt()
        .with_level(true)
        .with_thread_ids(true)
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .finish()
        .init();
}

// TODO : These should be command line options. Duh.
const NUM_STREAMS: usize = 32;
const NUM_CHANNELS: usize = 16;
const BYTE_LIMIT: i64 = 4 * 1024 * 1024 * 1024;
const TEST_DURATION: Duration = Duration::from_secs(36 * 60 * 60);
//const NUM_STREAMS: usize = 1;
//const NUM_CHANNELS: usize = 1;
//const BYTE_LIMIT: i64 = 4 * 1024 * 1024 * 1024;
//const TEST_DURATION: Duration = Duration::from_secs(60);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    enable_tracing();

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

    let mut tasks: JoinSet<(u64, google_cloud_pubsub::Result<()>)> = JoinSet::new();
    for i in 0..NUM_STREAMS {
        let cancel = cancel.clone();
        let client = client.clone();

        tracing::info!("Opening subscriber session {i}.");
        let mut session = client
            .streaming_pull("projects/dbolduc-test/subscriptions/subscription-id")
            .set_max_outstanding_messages(0)
            .set_max_outstanding_bytes(BYTE_LIMIT)
            .start();
        tracing::info!("Opened subscriber session {i}.");
        tasks.spawn(async move {
            let mut ack_count = 0;
            while let Some(r) = tokio::select! {
                v = session.next() => v,
                _ = cancel.cancelled() => None,
            } {
                let (_m, h) = match r {
                    Ok((m, h)) => (m, h),
                    Err(e) => return (ack_count, Err(e)),
                };
                //tracing::info!("Application received message={m:?}.");
                h.ack();
                ack_count += 1;
            }

            //let status = session.close().await;
            tracing::info!("Subscriber session {i} is closed.");
            (ack_count, Ok(()))
        });
    }
    let mut ack_count = 0;
    for t in tasks.join_all().await {
        ack_count += t.0;
        tracing::info!("Final stream status={:?}", t.1);
    }

    println!("Acked {ack_count} messages in {TEST_DURATION:?}, via {NUM_STREAMS} streams. Client had {NUM_CHANNELS} channels.");

    Ok(())
}
